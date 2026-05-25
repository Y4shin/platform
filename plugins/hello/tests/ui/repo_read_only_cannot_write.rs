//! A read-only repository (`HelloRead` witness) must not be able to call the
//! write method `create` — `create` requires `Has<HelloWrite>`, which the
//! witness doesn't prove. This is the compile-time half of the permission gate.

use hello_plugin::repo::{HelloRepo, NewGreeting};

type ReadOnly = junius_sdk::permissions!(hello_plugin::permissions::HelloRead);

async fn use_it(repo: HelloRepo<ReadOnly>) {
    let _ = repo
        .create(NewGreeting {
            name: String::new(),
            body: String::new(),
        })
        .await;
}

fn main() {}
