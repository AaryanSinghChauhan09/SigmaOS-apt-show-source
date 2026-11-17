// Integration test for SigmaOS-apt-show-source
// Verifies the crate builds and links as a dependency.
#![allow(unused_imports)]

use apt_show_source as _;

#[test]
fn crate_links_and_runs() {
    assert_eq!(2 + 2, 4);
}
