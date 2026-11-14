// 隐藏 Windows 下的控制台黑色终端窗口（调试时如果想看 println! 日志，可以注释掉这行）
#![windows_subsystem = "windows"]

use chrono::{DateTime, Local, NaiveDateTime, TimeZone};
use eframe::egui;
use std::fs::{FileTimes, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

// 引入 Windows 平台专属的文件扩展 Trait：
// 1. FileTimesExt: 为 std::fs::FileTimes 提供 Windows 独有的 set_created()（设置创建时间）方法
// 2. OpenOptionsExt: 为 std::fs::OpenOptions 提供底层 Win32 CreateFile 标志位配置能力
#[cfg(windows)]
use std::os::windows::fs::{FileTimesExt, OpenOptionsExt};

fn main() -> eframe::Result<()> {
    // 1. 配置桌面窗口的基本属性
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([580.0, 580.0])     // 初始窗口大小
            .with_min_inner_size([600.0, 450.0]) // 允许缩放的最小窗口尺寸
            .with_title("Windows 文件时间戳修改工具"),
        ..Default::default()
    };

    // 2. 启动 eframe 渲染与事件主循环
    eframe::run_native(
        "FileTimeModifierApp",
        native_options,
        Box::new(|cc| {
            // 注入 Windows 系统默认中文字体，防止中文显示为方框乱码
            setup_custom_fonts(&cc.egui_ctx);
            Ok(Box::new(FileTimeApp::default()))
        }),
    )
}

/// 应用程序核心状态结构体（保存界面上的所有输入值和选项开关）
struct FileTimeApp {
    /// 待处理的目标文件或文件夹路径列表（支持一次性拖入多个）
    target_paths: Vec<PathBuf>,

    /// 勾选开关：分别控制是否修改创建时间、修改时间、访问时间
    modify_creation: bool,
    modify_modification: bool,
    modify_access: bool,

    /// 目标时间输入框绑定的字符串（年、月、日、时、分、秒）
    year: String,
    month: String,
    day: String,
    hour: String,
    minute: String,
    second: String,

    /// 文件夹处理选项
    include_subfiles: bool,  // 是否递归遍历并修改文件夹下的所有子文件/子目录
    modify_dir_itself: bool, // 是否同时修改被拖入文件夹本身的时间戳

    /// 底部状态栏的提示文本与颜色状态
    status_message: String,
    status_is_error: bool,
}

impl Default for FileTimeApp {
    fn default() -> Self {
        // 启动时默认读取当前电脑本地时间填入输入框
        let now = Local::now();
        Self {
            target_paths: Vec::new(),
            modify_creation: true,
            modify_modification: true,
            modify_access: true,

            year: now.format("%Y").to_string(),
            month: now.format("%m").to_string(),
            day: now.format("%d").to_string(),
            hour: now.format("%H").to_string(),
            minute: now.format("%M").to_string(),
            second: now.format("%S").to_string(),

            include_subfiles: true,
            modify_dir_itself: true,

            status_message: "请将文件或文件夹拖入下方区域...".to_string(),
            status_is_error: false,
        }
    }
}

impl eframe::App for FileTimeApp {
    /// egui 的核心渲染回调（每一帧都会执行，类似游戏引擎的 Update 循环）
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // =========================================================================
        // 原理解释 1：监听 Windows 原生文件拖放事件 (Drag & Drop)
        // eframe 底层会接收 Win32 的 WM_DROPFILES 消息，并将其封装进 ctx.input().raw.dropped_files。
        // 只要检测到用户松开鼠标将文件拖入窗口，就把路径提取出来存入 target_paths 列表。
        // =========================================================================
        ctx.input(|i| {
            if !i.raw.dropped_files.is_empty() {
                for file in &i.raw.dropped_files {
                    if let Some(path) = &file.path {
                        // 去重：避免重复添加相同路径
                        if !self.target_paths.contains(path) {
                            self.target_paths.push(path.clone());
                        }
                    }
                }
                self.status_message = format!("已加载 {} 个项目", self.target_paths.len());
                self.status_is_error = false;
            }
        });

        // 顶层主面板容器
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("🕒 Windows 文件时间属性批量修改");
            ui.add_space(8.0);

            // --- 区域 1：时间设置卡片 ---
            ui.group(|ui| {
                ui.label(egui::RichText::new("1. 设定目标时间 (24小时制)").strong());
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.year).desired_width(50.0));
                    ui.label("年");
                    ui.add(egui::TextEdit::singleline(&mut self.month).desired_width(30.0));
                    ui.label("月");
                    ui.add(egui::TextEdit::singleline(&mut self.day).desired_width(30.0));
                    ui.label("日");
                    ui.separator();
                    ui.add(egui::TextEdit::singleline(&mut self.hour).desired_width(30.0));
                    ui.label("时");
                    ui.add(egui::TextEdit::singleline(&mut self.minute).desired_width(30.0));
                    ui.label("分");
                    ui.add(egui::TextEdit::singleline(&mut self.second).desired_width(30.0));
                    ui.label("秒");

                    // 快捷按钮：一键刷新为系统当前时间
                    if ui.button("🕒 设为当前时间").clicked() {
                        let now = Local::now();
                        self.year = now.format("%Y").to_string();
                        self.month = now.format("%m").to_string();
                        self.day = now.format("%d").to_string();
                        self.hour = now.format("%H").to_string();
                        self.minute = now.format("%M").to_string();
                        self.second = now.format("%S").to_string();
                    }
                });

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("选择修改的时间维度：");
                    ui.checkbox(&mut self.modify_creation, "创建时间");
                    ui.checkbox(&mut self.modify_modification, "修改时间");
                    ui.checkbox(&mut self.modify_access, "访问时间");
                });
            });

            ui.add_space(8.0);

            // --- 区域 2：文件夹处理规则卡片 ---
            ui.group(|ui| {
                ui.label(egui::RichText::new("2. 文件夹处理选项").strong());
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.include_subfiles, "修改目录下所有子文件及子目录");
                    ui.checkbox(&mut self.modify_dir_itself, "修改文件夹本身的时间");
                });
            });

            ui.add_space(8.0);

            // --- 区域 3：文件拖拽列表展示卡片 ---
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("3. 目标文件列表 (支持直接拖入文件/文件夹)").strong());
                    if ui.button("清空列表").clicked() {
                        self.target_paths.clear();
                        self.status_message = "列表已清空".into();
                        self.status_is_error = false;
                    }
                });

                // 可滚动的列表视图，防止拖入几百个文件时撑爆窗口高度
                egui::ScrollArea::vertical()
                    .max_height(180.0)
                    .show(ui, |ui| {
                        if self.target_paths.is_empty() {
                            ui.vertical_centered(|ui| {
                                ui.add_space(30.0);
                                ui.label(
                                    egui::RichText::new("【 将文件或文件夹直接拖拽至窗口任意位置 】")
                                        .weak(),
                                );
                                ui.add_space(30.0);
                            });
                        } else {
                            for (index, path) in self.target_paths.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    let icon = if path.is_dir() { "📁" } else { "📄" };
                                    ui.label(format!(
                                        "{}. {} {}",
                                        index + 1,
                                        icon,
                                        path.to_string_lossy()
                                    ));
                                });
                            }
                        }
                    });
            });

            ui.add_space(10.0);

            // --- 区域 4：执行操作按钮与状态栏 ---
            ui.horizontal(|ui| {
                // 只有当列表不为空，且至少勾选了一个时间维度时，按钮才允许点击
                let can_execute = !self.target_paths.is_empty()
                    && (self.modify_creation || self.modify_modification || self.modify_access);

                if ui
                    .add_enabled(
                        can_execute,
                        egui::Button::new(egui::RichText::new("🚀 开始修改时间戳").strong()),
                    )
                    .clicked()
                {
                    self.execute_modification();
                }

                if !can_execute && self.target_paths.is_empty() {
                    ui.label(egui::RichText::new("（请先拖入文件或文件夹）").weak());
                }
            });

            ui.add_space(8.0);
            let status_color = if self.status_is_error {
                egui::Color32::RED
            } else {
                egui::Color32::LIGHT_GREEN
            };
            ui.colored_label(status_color, &self.status_message);
        });
    }
}

impl FileTimeApp {
    /// 解析用户输入的本地时间，并批量执行文件/文件夹时间戳修改
    fn execute_modification(&mut self) {
        // 1. 将输入框的字符串格式化为标准时间字符串 (YYYY-MM-DD HH:MM:SS)
        let time_str = format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            self.year.trim().parse::<u32>().unwrap_or(2026),
            self.month.trim().parse::<u32>().unwrap_or(1),
            self.day.trim().parse::<u32>().unwrap_or(1),
            self.hour.trim().parse::<u32>().unwrap_or(0),
            self.minute.trim().parse::<u32>().unwrap_or(0),
            self.second.trim().parse::<u32>().unwrap_or(0),
        );

        // 2. 校验日期合法性（例如拦截 2月30日 等非法日期）
        let target_naive = match NaiveDateTime::parse_from_str(&time_str, "%Y-%m-%d %H:%M:%S") {
            Ok(dt) => dt,
            Err(e) => {
                self.status_message = format!("时间格式非法，请检查输入: {}", e);
                self.status_is_error = true;
                return;
            }
        };

        // 3. 将本地时区（如北京时间 UTC+8）转换为绝对时间戳
        let local_dt: DateTime<Local> = match Local.from_local_datetime(&target_naive).single() {
            Some(dt) => dt,
            None => {
                self.status_message = "本地时区转换失败，请检查时间格式".into();
                self.status_is_error = true;
                return;
            }
        };

        // 4. 转换为 Rust 标准库原生支持的 SystemTime
        let secs = local_dt.timestamp();
        let nanos = local_dt.timestamp_subsec_nanos();
        let target_systime = if secs >= 0 {
            UNIX_EPOCH + Duration::new(secs as u64, nanos)
        } else {
            // 支持设置 1970 年之前的早期时间戳
            UNIX_EPOCH - Duration::new((-secs) as u64, 0)
        };

        let mut success_count = 0usize;
        let mut fail_count = 0usize;

        // 5. 遍历处理用户拖入的每一个根路径
        for root_path in &self.target_paths {
            if root_path.is_file() {
                // 情况 A：单个普通文件
                if modify_single_entry(
                    root_path,
                    target_systime,
                    self.modify_creation,
                    self.modify_modification,
                    self.modify_access,
                ) {
                    success_count += 1;
                } else {
                    fail_count += 1;
                }
            } else if root_path.is_dir() {
                // 情况 B：文件夹
                // B1. 如果勾选了“递归修改子文件”，使用 WalkDir 深度遍历所有子项
                // 注意：WalkDir 默认按自顶向下顺序遍历；为避免修改子文件时触发父目录的“修改时间”自动更新，
                // 使用 contents_first(true) 让它先修改最深层的子文件，最后再修改外层目录！
                if self.include_subfiles {
                    for entry in WalkDir::new(root_path)
                        .contents_first(true)
                        .into_iter()
                        .filter_map(|e| e.ok())
                    {
                        let path = entry.path();
                        // 根目录本身留到下面的 B2 步骤单独判断处理
                        if path == root_path {
                            continue;
                        }
                        if modify_single_entry(
                            path,
                            target_systime,
                            self.modify_creation,
                            self.modify_modification,
                            self.modify_access,
                        ) {
                            success_count += 1;
                        } else {
                            fail_count += 1;
                        }
                    }
                }

                // B2. 如果勾选了“修改文件夹本身的时间”，在所有子项处理完毕后修改根目录本身
                if self.modify_dir_itself {
                    if modify_single_entry(
                        root_path,
                        target_systime,
                        self.modify_creation,
                        self.modify_modification,
                        self.modify_access,
                    ) {
                        success_count += 1;
                    } else {
                        fail_count += 1;
                    }
                }
            }
        }

        self.status_message = format!(
            "操作完成！成功修改: {} 个项目，失败: {} 个",
            success_count, fail_count
        );
        self.status_is_error = fail_count > 0;
    }
}

/// 修改单个文件或文件夹的时间戳（核心底层逻辑）
fn modify_single_entry(
    path: &Path,
    target_time: SystemTime,
    mod_create: bool,
    mod_write: bool,
    mod_access: bool,
) -> bool {
    #[cfg(windows)]
    {
        // =========================================================================
        // 原理解释 2：Win32 CreateFile 权限与标志位
        // 1. FILE_WRITE_ATTRIBUTES (0x0100)：
        //    向 Windows 申请“仅修改文件属性/时间戳”的最小权限。
        //    ⚠️ 极其重要：绝对不要在这里调用 .read(true) 或 .write(true)！
        //    因为 Rust 标准库一旦看到 .read(true)，就会把 access_mode 强制覆盖为 GENERIC_READ，
        //    从而把 FILE_WRITE_ATTRIBUTES 冲掉，导致后续写入时间失败！
        // 2. FILE_FLAG_BACKUP_SEMANTICS (0x02000000)：
        //    Windows 默认不允许像打开普通文件那样打开文件夹句柄。
        //    传入此标志位后，OpenOptions 既能打开普通文件，也能合法打开文件夹句柄，
        //    从而让文件夹的“创建日期”和“修改日期”也能被正常修改。
        // =========================================================================
        const FILE_WRITE_ATTRIBUTES: u32 = 0x0100;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x02000000;

        let file = match OpenOptions::new()
            .access_mode(FILE_WRITE_ATTRIBUTES)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(path)
        {
            Ok(f) => f,
            Err(_) => return false, // 如果文件被占用或无管理员权限（如系统盘受保护文件），返回 false
        };

        // =========================================================================
        // 原理解释 3：构造 FileTimes 结构体
        // Rust 原生的 FileTimes 对应 Win32 API SetFileTime(hFile, ctime, atime, mtime)。
        // 只有调用了对应 setter 的字段才会被更新，未勾选的维度会自动保留文件原有的时间戳，
        // 无需我们手动提前读取旧 metadata 再填回去。
        // =========================================================================
        let mut times = FileTimes::new();
        if mod_create {
            times = times.set_created(target_time); // 修改创建日期 (ftCreationTime)
        }
        if mod_write {
            times = times.set_modified(target_time); // 修改修改日期 (ftLastWriteTime)
        }
        if mod_access {
            times = times.set_accessed(target_time); // 修改访问日期 (ftLastAccessTime)
        }

        // 一次性将修改应用到文件系统
        file.set_times(times).is_ok()
    }

    #[cfg(not(windows))]
    {
        let _ = (path, target_time, mod_create, mod_write, mod_access);
        false
    }
}

/// 加载 Windows 系统内置中文字体（解决 egui 默认字体不支持中文导致显示为方框的问题）
fn setup_custom_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // 按优先级尝试读取 Windows 系统字体目录下的微软雅黑或宋体
    let font_paths = [
        "C:\\Windows\\Fonts\\msyh.ttc",   // 微软雅黑 (Win10/Win11 默认)
        "C:\\Windows\\Fonts\\simsun.ttc", // 宋体
        "C:\\Windows\\Fonts\\msyh.ttf",
    ];

    for path in font_paths {
        if let Ok(font_data) = std::fs::read(path) {
            fonts.font_data.insert(
                "windows_font".to_owned(),
                egui::FontData::from_owned(font_data),
            );

            // 将加载的中文字体插入到默认比例字体和等宽字体家族的最前面（最高优先级）
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "windows_font".to_owned());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .insert(0, "windows_font".to_owned());
            break;
        }
    }

    ctx.set_fonts(fonts);
}