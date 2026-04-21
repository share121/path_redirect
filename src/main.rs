#![windows_subsystem = "windows"]

use anyhow::Context;
use fs_err as fs;
use std::env;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

fn main() -> anyhow::Result<()> {
    if let Err(e) = link() {
        eprintln!("连接目录失败: {e:?}");
    }
    let exe_path = env::current_exe()?;
    let lx_exe_path = exe_path.with_file_name("lx-music-desktop.exe");
    if lx_exe_path.exists() {
        let mut command = Command::new(lx_exe_path);
        if let Some(parent) = exe_path.parent() {
            command.current_dir(parent);
        }
        command.spawn()?;
        println!("程序已启动，助手即将退出。");
    } else {
        eprintln!(
            "错误：找不到主程序 {}，请确认助手放在软件根目录下。",
            lx_exe_path.display()
        );
    }
    Ok(())
}

fn link() -> anyhow::Result<()> {
    let appdata = env::var("APPDATA").context("无法获取 APPDATA 变量")?;
    let link_path = PathBuf::from(appdata).join("lx-music-desktop");
    let target_path = env::current_exe()
        .context("无法获取自身路径")?
        .with_file_name("lx-data");
    if !target_path.try_exists()? {
        fs::create_dir_all(&target_path)?;
        println!("创建目标目录: {}", target_path.display());
    }
    if link_path.try_exists()? {
        if link_path.is_symlink() {
            println!("检测到已映射的联接，正在更新映射关系...");
            fs::remove_dir(&link_path)?;
        } else if link_path.is_dir() {
            println!("检测到旧数据文件夹，正在迁移...");
            copy_dir_all(&link_path, &target_path)?;
            fs::remove_dir_all(&link_path)?;
            println!("迁移完成。");
        } else {
            fs::remove_file(&link_path)?;
        }
    }
    junction::create(&target_path, &link_path)?;
    println!(
        "成功将 {} 映射到 {}",
        link_path.display(),
        target_path.display()
    );
    Ok(())
}

/// 递归拷贝文件夹内容的辅助函数
fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> std::io::Result<()> {
    fs::create_dir_all(&dst)?;
    for entry in fs::read_dir(src.as_ref())? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}
