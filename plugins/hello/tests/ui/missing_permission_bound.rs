//! A `HelloRead`-only witness must not satisfy a `Has<HelloWrite>` bound — this
//! is the compile-time gate that stops a read-scoped context from reaching a
//! write-scoped repository method.

use hello_plugin::permissions::HelloWrite;
use junius_sdk::permissions::Has;

fn needs_write<P: Has<HelloWrite, I>, I>() {}

fn main() {
    // permissions!(HelloRead) = And<HelloRead, ()>, which does not contain HelloWrite.
    needs_write::<junius_sdk::permissions!(hello_plugin::permissions::HelloRead), _>();
}
