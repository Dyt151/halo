//! 进程入口。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    halo_lib::run();
}
