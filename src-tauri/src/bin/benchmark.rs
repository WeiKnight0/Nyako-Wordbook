use nyako_wordbook_lib::database;
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::temp_dir().join(format!("nyako-benchmark-{}.sqlite3", std::process::id()));
    let result = (|| -> Result<(), String> {
        let started = Instant::now();
        database::initialize(&path, 200_000).map_err(|e| e.message)?;
        println!(
            "200000 synthetic words: {:.2}s",
            started.elapsed().as_secs_f64()
        );
        for query in ["工", "工作", "ハタラク", "はたらく 工作", "存在しない"] {
            let mut times = Vec::new();
            for _ in 0..100 {
                let page = database::search(&path, query, 0, 100).map_err(|e| e.message)?;
                times.push(page.elapsed_ms);
            }
            times.sort_by(f64::total_cmp);
            println!("{query}: backend P95 {:.2}ms", times[94]);
        }
        Ok(())
    })();
    for suffix in ["", "-wal", "-shm"] {
        let file = format!("{}{suffix}", path.display());
        if std::path::Path::new(&file).exists() {
            std::fs::remove_file(file)?;
        }
    }
    result.map_err(Into::into)
}
