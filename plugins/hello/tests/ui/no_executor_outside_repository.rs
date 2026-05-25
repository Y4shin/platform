//! Ordinary plugin code cannot run SQL: `PluginResources` hands out only an
//! opaque `PluginDb`, which is not a sqlx `Executor`, so a query can't be
//! pointed at it.

async fn use_it(resources: junius_sdk::PluginResources) {
    // `resources.db()` is a `&PluginDb` — opaque, not an executor.
    let _ = sqlx::query("SELECT 1").fetch_all(resources.db()).await;
}

fn main() {}
