#![feature(async_fn_in_trait)]
#![feature(never_type)]
#![feature(associated_type_defaults)]
#![feature(try_trait_v2)]
#![feature(try_blocks)]
// #![feature(return_position_impl_trait_in_trait)]
#![feature(error_generic_member_access)]
#![feature(provide_any)]

//! # anystore
//!
//! `anystore` is a polymorphic, type-safe, composable async framework for specifying API for arbitrary stores
//! (including databases and configuration systems). It supports addressing arbitrary type-safe hierarchies of objects.
//!
//! It is best used for prototyping and configuration. It is especially good for situations when a storage system is needed,
//! but it's not very important, and you want to be able to change the storage provider quickly, in case the requirements change.
//!
//! It is good for when you don't want to learn *yet another API*. (It also might be good if you don't want to invent *yet another API*, as it gives enough structure, primitives and utils to help you build a decent client.)
//!
//! It is not indended to be used when you need high performance or reliability.
//!
//! **This crate is nightly-only**. It heavily depends on `async_fn_in_trait` feature, which is not stable yet.
//!
//! Goals:
//!
//! * Provide many reasonable defaults and demand very little inconsequential choices.
//! * Provide a variety of wrappers to compose or augment particular stores.
//! * Type safety: as long as the type is statically determinable, it should be inferred.
//! * Work nicely with autocompeletion and type inference.
//! * Generic dynamic configuration for any store (see [`Metastore`]).
//! * Support CLI/TUI tools for navigating and editing arbitrary stores.
//! * Make it easy and safe to switch the store used by a program.
//! * Become a reasonable framework for developing API interfaces for stores.
//! * Provide an extensive test framework to make testing of new store implementations easier.
//! * Try to capture that blessed feeling of wonder when you implement a few small pieces and suddenly
//!     you have a lot of cool emerging features.
//! * All stores should be thread-safe, Sync and Send.
//!
//!
//! Non-goals:
//!
//! * Serious performance considerations beyond simple hygiene. This will never be a goal.
//! * Supporting all the database features for a particular database. We'd rather support the most general
//!     useful subset.
//! * For now, avoiding unsafe/unstable features. This crate already uses `async_fn_in_trait`,
//!     and while it's not stable, there's really no point in trying to keep everything `stable`.
//!     Hopefully one day it'll stabilize and we can rethinking this non-goal.

//! # Quick example
//!
//! todo
//!
//! check out: [`Location::walk_tree_recursively`](location::Location::walk_tree_recursively)
//!
//! # Main concepts
//!
//! ## Address
//!
//! A storage system is defined around the concept of [`address::Address`]. Address uniquely identifies
//! a piece of content. A storage system can support several types as [Addresses][`address::Address`]: e.g.
//! pointers to specific databases, tables, rows, cells, or even subvalues inside cells.
//!
//! If a system understands a particular address, it implements [traits][`address::traits`] like [`address::traits::AddressableRead`]. `address::traits::AddressableRead<SomeType, SomeAddr>` means that `SomeAddr` can be used to read a value of `SomeType`. Often there's a bunch of `SomeType`s that are useable with an address, including special values like `address::primitive::Existence` which is simply used to check whether something exists at that address.
//!
//! In some cases, "address" and "value" are more or less the same thing. E.g., if you list your Airtable bases, you get the data about them, but then you can reuse it as an address. (?)
//!
//! ## Location
//!
//! [`location::Location`] is simply a pair of an address and a store. This is the value you'd most typically pass around,
//! and it has a bunch of helper methods. You can traverse it further:
//!
#![cfg_attr(not(feature = "json"), doc = "```ignore")]
#![cfg_attr(feature = "json", doc = "```no_run")]
//! # use anystore::stores::json::*;
//! # use anystore::location::Location;
//! # fn testloc(jsonlocation: Location<JsonPath, JsonValueStore>) {
//! let location = jsonlocation.sub(JsonPathPart::Key("subkey".to_owned())).sub(JsonPathPart::Index(2));
//! # }
//! ```
//!
//! At every point, `Location` tracks the type and is typically able to statically tell you what kind of
//! values you can expect there. `Store`s can define an [`DefaultValue`](address::Addressable::DefaultValue) for the addresses,
//! which allows you to get immediately the correct default type:
//!
#![cfg_attr(not(feature = "json"), doc = "```ignore")]
#![cfg_attr(feature = "json", doc = "```no_run")]
//! # use anystore::location::Location;
//! # use anystore::stores::json::*;
//! # use anystore::store::*;
//! # use anystore::address::traits::*;
//! # async fn test<V, S: Store + Addressable<JsonPath, DefaultValue=V> + AddressableRead<V, JsonPath>>(location: Location<JsonPath, S>) -> StoreResult<(), S> {
//! let val = location.getv().await?;
//! # Ok(()) };
//! ```
//!
//! In cases when you need a more refined type than the default, you can use e.g. [`Location::get::<Type>`](location::Location::get), as long as
//! the address supports that type.
//!
#![cfg_attr(not(feature = "json"), doc = "```ignore")]
#![cfg_attr(feature = "json", doc = "```no_run")]
//! # use anystore::store::StoreResult;
//! # use anystore::location::Location;
//! # use anystore::store::Store;
//! # use anystore::address::traits::*;
//! # use anystore::stores::json::*;
//! # type Value = ();
//! # async fn test<S: Store + AddressableRead<Value, JsonPath>>(location: Location<JsonPath, S>) -> StoreResult<(), S> {
//! let val = location.get::<Value>().await?;
//! # Ok(()) };
//! ```
//!
//! In many cases, you can also use strings to traverse it, which is sometimes convenient but less typesafe.
//!
#![cfg_attr(not(feature = "json"), doc = "```ignore")]
#![cfg_attr(feature = "json", doc = "```no_run")]
//! # use anystore::stores::json::*;
//! # use anystore::store::StoreResult;
//! # use anystore::location::Location;
//! # fn testloc(jsonlocation: Location<JsonPath, JsonValueStore>) -> StoreResult<(), JsonValueStore> {
//! let location = jsonlocation.path("subkey[2]")?.path("deeper.anotherone[12]")?;
//! # Ok(()) };
//! ```
//!
//! ## Wrappers
//!
//! The traits in this crate are designed to be easily composable without too much boilerplate. That allows
//! to create abstract wrappers that add functionality to the existing stores, or compose stores together.
//!
//! # Supported stores
//!
//! The supported stores:
//!
//! Memory:
//! - [`stores::json::JsonValueStore`]
//! - tree of values
//!
//! File databases:
//! - file system (see also wrappers)
//! - RocksDB
//!
//! Network-accessible databases:
//! - Redis
//!
//! Cloud services:
//! - [`stores::cloud::airtable::AirtableStore`]
//!
//! Configuration tools:
//! - Kubernetes
//!
//! Wrappers:
//! - json value wrapper
//! - rate limiter
//! - ignore keys wrapper (through into?..)
//!
pub mod store;

pub mod address;
pub mod location;
pub mod stores;
pub mod util;
pub mod wrappers;
