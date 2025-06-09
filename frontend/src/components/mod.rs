pub mod control_panel;
pub mod file_tree;
pub mod content_view;
pub mod raw_view;

pub use control_panel::ControlPanel;
pub use file_tree::FileTreeView;
pub use content_view::ContentView;
pub use raw_view::RawView;

// Helper functions
pub fn format_size(bytes: usize) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;
    
    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }
    
    format!("{:.1} {}", size, UNITS[unit_index])
}

pub fn format_tokens(tokens: usize) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}k", tokens as f64 / 1_000.0)
    } else {
        tokens.to_string()
    }
}

pub fn get_file_icon(filename: &str) -> &'static str {
    let ext = filename.split('.').last().unwrap_or("");
    match ext {
        "rs" => "🦀",
        "js" | "jsx" => "📜",
        "ts" | "tsx" => "📘",
        "py" => "🐍",
        "go" => "🐹",
        "java" => "☕",
        "c" | "cpp" | "cc" => "⚙️",
        "h" | "hpp" => "📎",
        "md" => "📝",
        "json" => "📊",
        "toml" | "yaml" | "yml" => "⚙️",
        "html" => "🌐",
        "css" | "scss" | "sass" => "🎨",
        "png" | "jpg" | "jpeg" | "gif" | "svg" => "🖼️",
        "lock" => "🔒",
        _ => "📄",
    }
}
