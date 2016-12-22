// Copyright 2016 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under (1) the MaidSafe.net Commercial License,
// version 1.0 or later, or (2) The General Public License (GPL), version 3, depending on which
// licence you accepted on initial access to the Software (the "Licences").
//
// By contributing code to the SAFE Network Software, or to this project generally, you agree to be
// bound by the terms of the MaidSafe Contributor Agreement, version 1.1.  This, along with the
// Licenses can be found in the root directory of this project at LICENSE, COPYING and CONTRIBUTOR.
//
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.
//
// Please review the Licences for the specific language governing permissions and limitations
// relating to use of the SAFE Network Software.

#![cfg(feature = "use-mock-crust")]

// For explanation of lint checks, run `rustc -W help` or see
// https://github.com/maidsafe/QA/blob/master/Documentation/Rust%20Lint%20Checks.md
#![forbid(bad_style, exceeding_bitshifts, mutable_transmutes, no_mangle_const_items,
          unknown_crate_types, warnings)]
#![deny(deprecated, improper_ctypes, missing_docs,
        non_shorthand_field_patterns, overflowing_literals, plugin_as_library,
        private_no_mangle_fns, private_no_mangle_statics, stable_features, unconditional_recursion,
        unknown_lints, unsafe_code, unused, unused_allocation, unused_attributes,
        unused_comparisons, unused_features, unused_parens, while_true)]
#![warn(trivial_casts, trivial_numeric_casts, unused_extern_crates, unused_import_braces,
        unused_qualifications, unused_results)]
#![allow(box_pointers, fat_ptr_transmutes, missing_copy_implementations,
         missing_debug_implementations, variant_size_differences)]

#![cfg_attr(feature="clippy", feature(plugin))]
#![cfg_attr(feature="clippy", plugin(clippy))]
#![cfg_attr(feature="clippy", deny(clippy, unicode_not_nfc, wrong_pub_self_convention,
                                   option_unwrap_used))]
#![cfg_attr(feature="clippy", allow(use_debug))]

extern crate itertools;
#[macro_use]
extern crate log;
#[cfg_attr(feature="clippy", allow(useless_attribute))]
#[allow(unused_extern_crates)]
#[macro_use]
extern crate maidsafe_utilities;
extern crate rand;
extern crate routing;
#[macro_use]
extern crate unwrap;

// This module is a driver and defines macros. See `mock_crust` modules for
// tests.

/// Expect that the next event raised by the node matches the given pattern.
/// Panics if no event, or an event that does not match the pattern is raised.
/// (ignores ticks).
macro_rules! expect_next_event {
    ($node:expr, $pattern:pat) => {
        loop {
            match $node.event_rx.try_recv() {
                Ok($pattern) => break,
                Ok(Event::Tick) => (),
                other => panic!("Expected Ok({}) at {}, got {:?}",
                    stringify!($pattern),
                    unwrap!($node.inner.name()),
                    other),
            }
        }
    }
}

/// Expects that any event raised by the node matches the given pattern
/// (with optional pattern guard). Ignores events that do not match the pattern.
/// Panics if the event channel is exhausted before matching event is found.
macro_rules! expect_any_event {
    ($node:expr, $pattern:pat) => {
        expect_any_event!($node, $pattern if true => ())
    };
    ($node:expr, $pattern:pat if $guard:expr) => {
        loop {
            match $node.event_rx.try_recv() {
                Ok($pattern) if $guard => break,
                Ok(_) => (),
                other => panic!("Expected Ok({}) at {}, got {:?}",
                    stringify!($pattern),
                    unwrap!($node.inner.name()),
                    other),
            }
        }
    }
}

/// Expects that the node raised no event, panics otherwise (ignores ticks).
macro_rules! expect_no_event {
    ($node:expr) => {
        match $node.event_rx.try_recv() {
            Ok(Event::Tick) => (),
            Err(mpsc::TryRecvError::Empty) => (),
            other => panic!("Expected no event at {}, got {:?}",
                unwrap!($node.inner.name()),
                other),
        }
    }
}

/// Checks that an expression is true.
/// Copied from libcore/macros.rs, with minor changes
macro_rules! check {
    ($cond:expr) => (
        if !$cond {
            let msg = String::from(concat!("check failed: ", stringify!($cond)));
            return Err(CheckError::CheckFailure(file!(), line!(), column!(), msg));
        }
    );
    ($cond:expr, $($arg:tt)+) => (
        if !$cond {
            let msg = format!($($arg)+);
            return Err(CheckError::CheckFailure(file!(), line!(), column!(), msg));
        }
    );
}

/// Checks that two expressions are equal.
/// Copied from libcore/macros.rs, with minor changes
macro_rules! check_eq {
    ($left:expr , $right:expr) => ({
        match (&$left, &$right) {
            (left_val, right_val) => {
                if !(*left_val == *right_val) {
                    let msg = format!("check failed: `(left == right)` \
                           (left: `{:?}`, right: `{:?}`)", left_val, right_val);
                    return Err(CheckError::CheckFailure(file!(), line!(), column!(), msg));
                }
            }
        }
    });
    ($left:expr , $right:expr, $($arg:tt)*) => ({
        match (&($left), &($right)) {
            (left_val, right_val) => {
                if !(*left_val == *right_val) {
                    let msg = format!("check failed: `(left == right)` \
                           (left: `{:?}`, right: `{:?}`): {}", left_val, right_val,
                           format_args!($($arg)*));
                    return Err(CheckError::CheckFailure(file!(), line!(), column!(), msg));
                }
            }
        }
    });
}

mod mock_crust;

use routing::{InterfaceError, RoutingError};


// -----  Error types  -----

/// Generic error type for `check` macros and wrapped errors.
///
/// TODO: it may be useful to include file/line numbers for wrapped errors. This would require
/// using a custom macro in place of the std `try!` / `?`. (Can we simply redefine `try!`?)
enum CheckError {
    CheckFailure(&'static str, u32, u32, String),
    Interface(InterfaceError),
    Routing(RoutingError),
}

impl CheckError {
    /// Print details
    fn println(&self) {
        use CheckError::*;
        match *self {
            CheckFailure(file, line, col, ref msg) => {
                println!("{}:{}:{}: {}", file, line, col, msg)
            }
            Interface(ref e) => println!("{:?}", e),
            Routing(ref e) => println!("{:?}", e),
        };
    }
}

impl From<InterfaceError> for CheckError {
    fn from(error: InterfaceError) -> CheckError {
        CheckError::Interface(error)
    }
}

impl From<RoutingError> for CheckError {
    fn from(error: RoutingError) -> CheckError {
        CheckError::Routing(error)
    }
}

type CheckResult<T> = Result<T, CheckError>;