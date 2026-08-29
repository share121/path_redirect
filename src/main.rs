use anyhow::Context;
use clap::Parser;
use fs_err as fs;
use std::env;
use std::path::Path;
use std::process::Command;

/// 将软件数据目录重定向到外部存储，并启动主程序
#[derive(Parser)]
#[command(about)]
struct Args {
    /// 主程序可执行文件名（相对 helper 所在目录）
    target_exe_path: String,
    /// 原始数据目录（将被重定向）
    src_dir: String,
    /// 目标数据目录（实际存储位置）
    dst_dir: String,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let target_exe_path = &args.target_exe_path;
    let src_dir = &args.src_dir;
    let dst_dir = &args.dst_dir;

    let self_exe = env::current_exe()?;
    let self_dir = self_exe.parent().context("无法获取程序运行目录")?;

    if let Err(e) = link(src_dir, dst_dir) {
        eprintln!("连接目录失败: {e:?}");
    }
    let target_exe_path = self_dir.join(target_exe_path);
    if fs::exists(&target_exe_path).unwrap_or(false) {
        let mut command = Command::new(&target_exe_path);
        if let Some(exe_dir) = target_exe_path.parent() {
            command.current_dir(exe_dir);
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
            println!("正在迁移数据...");
            move_dir(src, dst).context("数据迁移失败")?;
            println!("迁移完成");
        } else {
            fs::remove_file(src).context("路径被文件占用，且无法删除")?;
        }
    }
    junction::create(dst, src).context("建立目录联接失败")?;
    println!("已成功建立关联: {} -> {}", src.display(), dst.display());
    Ok(())
}

/// 调用系统 Robocopy 迁移数据
fn move_dir(src: &Path, dst: &Path) -> anyhow::Result<()> {
    #[allow(clippy::unreadable_literal)]
    let status = Command::new("robocopy")
        .arg(src)
        .arg(dst)
        .arg("/E") // 递归拷贝，包含子目录
        .arg("/MOVE") // 移动文件（成功后会自动删除源文件和目录）
        .arg("/XJ") // 排除联接点，防止死循环
        .arg("/MT:8") // 8线程开启
        .arg("/R:3") // 失败重试3次
        .arg("/W:1") // 间隔1秒
        .status()?;
    if status.code().unwrap_or(8) < 8 {
        Ok(())
    } else {
        anyhow::bail!("Robocopy 迁移任务失败，退出码: {:?}", status.code())
    }
}
