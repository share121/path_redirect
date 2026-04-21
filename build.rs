fn main() {
    let mut res = winres::WindowsResource::new();
    res.set_icon("app_icon.ico");
    res.set("FileDescription", "洛雪音乐数据映射助手");
    res.set("ProductName", "LX Music Helper");
    res.set("OriginalFilename", "lx-helper.exe");
    res.set("LegalCopyright", "Copyright (c) 2024");
    res.compile().unwrap();
}
