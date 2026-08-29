use anyhow::Context;
use clap::Parser;
use encoding_rs::GBK;
use fs_err as fs;
use std::env;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;

/// 将软件数据目录重定向到外部存储，并启动主程序
#[derive(Parser)]
#[command(version, about)]
struct Args {
    /// 主程序可执行文件名（相对 helper 所在目录）
    target_exe_path: String,
    /// 重定向映射：成对出现 <原始数据目录> <目标数据目录>，可重复
    #[arg(num_args = 2.., value_names = ["SRC_DIR", "DST_DIR"], required = true)]
    map: Vec<String>,
    /// 透传给主程序的额外参数，写在 -- 之后
    /// 例如：helper.exe target s1 d1 -- --proxy=127.0.0.1 --no-sandbox
    #[arg(last = true, num_args = 0.., value_name = "ARGS")]
    passthrough: Vec<String>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    if args.map.len() % 2 != 0 {
        anyhow::bail!("重定向映射必须成对出现（原始数据目录 + 目标数据目录）");
    }

    let self_exe = env::current_exe()?;
    let self_dir = self_exe.parent().context("无法获取程序运行目录")?;

    for [src_dir, dst_dir] in args.map.as_chunks::<2>().0 {
        let src_dir = self_dir.join(src_dir);
        let dst_dir = self_dir.join(dst_dir);
        if let Err(e) = link(src_dir, dst_dir) {
            eprintln!("连接目录失败: {e:?}");
        }
    }

    let target_exe_path = self_dir.join(&args.target_exe_path);
    if fs::exists(&target_exe_path).unwrap_or(false) {
        let mut command = Command::new(&target_exe_path);
        if let Some(exe_dir) = target_exe_path.parent() {
            command.current_dir(exe_dir);
        }
        for arg in &args.passthrough {
            command.arg(arg);
        }
        if let Err(e) = command.spawn() {
            eprintln!("无法启动主程序: {e:?}");
        }
    } else {
        eprintln!(
            "错误：找不到主程序 {}\n请确认助手放在软件根目录下。",
            target_exe_path.display()
        );
    }
    Ok(())
}

fn link(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> anyhow::Result<()> {
    let src = src.as_ref();
    let dst = dst.as_ref();
    if !fs::exists(dst).unwrap_or(false) {
        fs::create_dir_all(dst).context("无法创建目标存储目录")?;
    }
    if fs::exists(src).unwrap_or(false) {
        let metadata = fs::symlink_metadata(src)?;
        if metadata.is_symlink() || junction::exists(src).unwrap_or(false) {
            fs::remove_dir(src).context("无法移除旧的联接点")?;
        } else if metadata.is_dir() {
            eprintln!("正在迁移数据...");
            move_dir(src, dst).context("数据迁移失败")?;
            eprintln!("迁移完成");
        } else {
            fs::remove_file(src).context("路径被文件占用，且无法删除")?;
        }
    }
    junction::create(dst, src).context("建立目录联接失败")?;
    eprintln!("已成功建立关联: {} -> {}", src.display(), dst.display());
    Ok(())
}

/// 实时把子进程的 GBK 字节流转成 UTF-8 写到当前进程的对应流。
/// 使用流式解码器，保证一个多字节汉字被管道缓冲切到两块之间时也能完整解码。
fn pump<R: Read + Send + 'static>(mut reader: R) {
    let mut buf = [0u8; 4096];
    loop {
        match reader.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                let (text, _) = GBK.decode_without_bom_handling(&buf[..n]);
                if !text.is_empty() {
                    eprint!("{text}");
                    let _ = std::io::stderr().flush();
                }
            }
        }
    }
}

/// 调用系统 Robocopy 迁移数据
///
/// Robocopy 按系统代码页（中文 Windows 为 GBK/CP936）输出，这里通过管道实时捕获
/// 并流式转成 UTF-8 再写回日志，避免 log.txt 同时混入 UTF-8 与 GBK 而乱码。
fn move_dir(src: &Path, dst: &Path) -> anyhow::Result<()> {
    let mut child = Command::new("robocopy")
        .arg(src)
        .arg(dst)
        .arg("/E") // 递归拷贝，包含子目录
        .arg("/MOVE") // 移动文件（成功后会自动删除源文件和目录）
        .arg("/XJ") // 排除联接点，防止死循环
        .arg("/MT:8") // 8线程开启
        .arg("/R:3") // 失败重试3次
        .arg("/W:1") // 间隔1秒
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("无法启动 Robocopy")?;

    let stdout = child.stdout.take().context("无法获取 Robocopy stdout")?;
    let stderr = child.stderr.take().context("无法获取 Robocopy stderr")?;
    let t_out = thread::spawn(move || pump(stdout));
    let t_err = thread::spawn(move || pump(stderr));
    t_out.join().ok();
    t_err.join().ok();

    let status = child.wait().context("等待 Robocopy 结束失败")?;
    if status.code().unwrap_or(8) < 8 {
        Ok(())
    } else {
        anyhow::bail!("Robocopy 迁移任务失败，退出码: {:?}", status.code())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn passthrough_collects_args_after_double_dash() {
        let args = Args::try_parse_from([
            "path_redirect",
            "app.exe",
            "s1",
            "d1",
            "s2",
            "d2",
            "--",
            "--proxy=127.0.0.1",
            "--flag",
        ])
        .unwrap();
        assert_eq!(args.target_exe_path, "app.exe");
        assert_eq!(
            args.map,
            vec![
                "s1".to_string(),
                "d1".to_string(),
                "s2".to_string(),
                "d2".to_string()
            ]
        );
        assert_eq!(
            args.passthrough,
            vec!["--proxy=127.0.0.1".to_string(), "--flag".to_string()]
        );
    }

    #[test]
    fn no_passthrough_when_double_dash_absent() {
        let args = Args::try_parse_from(["path_redirect", "app.exe", "s1", "d1"]).unwrap();
        assert_eq!(args.map, vec!["s1".to_string(), "d1".to_string()]);
        assert_eq!(args.passthrough, Vec::<String>::new());
    }
}
