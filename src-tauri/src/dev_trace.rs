pub fn dev_trace(scope: &str, message: impl AsRef<str>) {
    if cfg!(debug_assertions) {
        eprintln!("[NineClaw][{scope}] {}", message.as_ref());
    }
}

#[allow(dead_code)]
pub fn dev_trace_block(scope: &str, label: impl AsRef<str>, content: impl AsRef<str>) {
    if !cfg!(debug_assertions) {
        return;
    }

    let label = label.as_ref();
    let content = content.as_ref();

    eprintln!("[NineClaw][{scope}] {label} BEGIN");
    if content.trim().is_empty() {
        eprintln!("[NineClaw][{scope}] <empty>");
    } else {
        for line in content.lines() {
            eprintln!("[NineClaw][{scope}] {line}");
        }
    }
    eprintln!("[NineClaw][{scope}] {label} END");
}
