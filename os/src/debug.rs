//! Utilities for debugging.

use alloc::vec::Vec;
use core::fmt::Display;

/// Print debug information for a Vec.
#[allow(unused)]
pub fn print_vec<T: Display>(vec: &Vec<T>) {
    for e in vec.iter() {
        print!("{} ", e);
    }
    println!("");
}

/// Print debug information for a 2-dimensional Vec.
#[allow(unused)]
pub fn print_vec_2d<T: Display>(vec: &Vec<Vec<T>>) {
    for r in vec.iter() {
        for c in r.iter() {
            print!("{} ", c);
        }
        println!("");
    }
}