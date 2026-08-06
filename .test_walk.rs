use std::path::Path;
use std::sync::Mutex;

#[test]
fn test_walkdir_speed() {
    let path = Path::new("TestDocWorld/external-projects/mobile-app");
    let count = std::sync::Mutex::new(0u32);
    let _ = walkdir::WalkDir::new(path)
        .into_iter()
        .filter_entry(|e| {
            let path_str = e.path().to_string_lossy();
            !path_str.contains("/.git") && !path_str.contains("node_modules")
        })
        .try_for_each(|e| {
            if let Ok(entry) = e {
                if entry.file_type().is_file() {
                    let mut c = count.lock().unwrap();
                    *c += 1;
                }
            }
            Ok::<(), std::io::Error>(())
        });
    println!("Files found: {}", count.lock().unwrap());
}
