// This file is part of Substrate.

// Copyright (C) 2022 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Autogenerated weights for pallet_nomination_pools
//!
//! THIS FILE WAS AUTO-GENERATED USING THE SUBSTRATE BENCHMARK CLI VERSION 4.0.0-dev
//! DATE: 2022-02-23, STEPS: `50`, REPEAT: 20, LOW RANGE: `[]`, HIGH RANGE: `[]`
//! EXECUTION: Some(Wasm), WASM-EXECUTION: Compiled, CHAIN: Some("dev"), DB CACHE: 1024

// Executed Command:
// target/production/substrate
// benchmark
// --chain=dev
// --steps=50
// --repeat=20
// --pallet=pallet_nomination_pools
// --extrinsic=*
// --execution=wasm
// --wasm-execution=compiled
// --heap-pages=4096
// --output=./frame/nomination-pools/src/weights.rs
// --template=./.maintain/frame-weight-template.hbs

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]

use frame_support::{traits::Get, weights::{Weight, constants::RocksDbWeight}};
use sp_std::marker::PhantomData;

/// Weight functions needed for pallet_nomination_pools.
pub trait WeightInfo {
	fn join() -> Weight;
	fn claim_payout() -> Weight;
	fn unbond_other() -> Weight;
	fn pool_withdraw_unbonded(s: u32, ) -> Weight;
	fn withdraw_unbonded_other_update(s: u32, ) -> Weight;
	fn withdraw_unbonded_other_kill(s: u32, ) -> Weight;
	fn create() -> Weight;
	fn nominate() -> Weight;
}

/// Weights for pallet_nomination_pools using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	// Storage: NominationPools MinJoinBond (r:1 w:0)
	// Storage: NominationPools Delegators (r:1 w:1)
	// Storage: NominationPools BondedPools (r:1 w:1)
	// Storage: Staking Ledger (r:1 w:1)
	// Storage: NominationPools RewardPools (r:1 w:0)
	// Storage: System Account (r:2 w:1)
	// Storage: Staking Bonded (r:1 w:0)
	// Storage: Balances Locks (r:1 w:1)
	// Storage: BagsList ListNodes (r:3 w:3)
	// Storage: BagsList ListBags (r:2 w:2)
	// Storage: NominationPools CounterForDelegators (r:1 w:1)
	fn join() -> Weight {
		(109_762_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(15 as Weight))
			.saturating_add(T::DbWeight::get().writes(11 as Weight))
	}
	// Storage: NominationPools Delegators (r:1 w:1)
	// Storage: NominationPools BondedPools (r:1 w:0)
	// Storage: NominationPools RewardPools (r:1 w:1)
	// Storage: System Account (r:1 w:1)
	fn claim_payout() -> Weight {
		(45_848_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(4 as Weight))
			.saturating_add(T::DbWeight::get().writes(3 as Weight))
	}
	// Storage: NominationPools Delegators (r:1 w:1)
	// Storage: NominationPools BondedPools (r:1 w:1)
	// Storage: NominationPools RewardPools (r:1 w:1)
	// Storage: System Account (r:2 w:1)
	// Storage: NominationPools SubPoolsStorage (r:1 w:1)
	// Storage: Staking CurrentEra (r:1 w:0)
	// Storage: Staking Ledger (r:1 w:1)
	// Storage: Staking Nominators (r:1 w:0)
	// Storage: Staking MinNominatorBond (r:1 w:0)
	// Storage: Balances Locks (r:1 w:1)
	// Storage: BagsList ListNodes (r:3 w:3)
	// Storage: Staking Bonded (r:1 w:0)
	// Storage: BagsList ListBags (r:2 w:2)
	// Storage: NominationPools CounterForSubPoolsStorage (r:1 w:1)
	fn unbond_other() -> Weight {
		(114_657_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(18 as Weight))
			.saturating_add(T::DbWeight::get().writes(13 as Weight))
	}
	// Storage: Staking Ledger (r:1 w:1)
	// Storage: Staking CurrentEra (r:1 w:0)
	// Storage: Balances Locks (r:1 w:1)
	fn pool_withdraw_unbonded(s: u32, ) -> Weight {
		(33_087_000 as Weight)
			// Standard Error: 0
			.saturating_add((50_000 as Weight).saturating_mul(s as Weight))
			.saturating_add(T::DbWeight::get().reads(3 as Weight))
			.saturating_add(T::DbWeight::get().writes(2 as Weight))
	}
	// Storage: NominationPools Delegators (r:1 w:1)
	// Storage: Staking CurrentEra (r:1 w:0)
	// Storage: NominationPools SubPoolsStorage (r:1 w:1)
	// Storage: NominationPools BondedPools (r:1 w:0)
	// Storage: Staking Ledger (r:1 w:1)
	// Storage: Balances Locks (r:1 w:1)
	// Storage: System Account (r:1 w:1)
	// Storage: NominationPools CounterForDelegators (r:1 w:1)
	fn withdraw_unbonded_other_update(s: u32, ) -> Weight {
		(66_522_000 as Weight)
			// Standard Error: 0
			.saturating_add((57_000 as Weight).saturating_mul(s as Weight))
			.saturating_add(T::DbWeight::get().reads(8 as Weight))
			.saturating_add(T::DbWeight::get().writes(6 as Weight))
	}
	fn withdraw_unbonded_other_kill(s: u32, ) -> Weight {
		(66_522_000 as Weight)
			// Standard Error: 0
			.saturating_add((57_000 as Weight).saturating_mul(s as Weight))
			.saturating_add(T::DbWeight::get().reads(8 as Weight))
			.saturating_add(T::DbWeight::get().writes(6 as Weight))
	}
	// Storage: Staking MinNominatorBond (r:1 w:0)
	// Storage: NominationPools MinCreateBond (r:1 w:0)
	// Storage: NominationPools MaxPools (r:1 w:0)
	// Storage: NominationPools CounterForBondedPools (r:1 w:1)
	// Storage: NominationPools Delegators (r:1 w:1)
	// Storage: System ParentHash (r:1 w:0)
	// Storage: unknown [0x3a65787472696e7369635f696e646578] (r:1 w:0)
	// Storage: NominationPools BondedPools (r:1 w:1)
	// Storage: Staking Ledger (r:1 w:1)
	// Storage: System Account (r:1 w:1)
	// Storage: Staking Bonded (r:1 w:1)
	// Storage: Staking CurrentEra (r:1 w:0)
	// Storage: Staking HistoryDepth (r:1 w:0)
	// Storage: Balances Locks (r:1 w:1)
	// Storage: NominationPools CounterForDelegators (r:1 w:1)
	// Storage: NominationPools RewardPools (r:1 w:1)
	// Storage: NominationPools CounterForRewardPools (r:1 w:1)
	// Storage: Staking Payee (r:0 w:1)
	fn create() -> Weight {
		(92_839_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(17 as Weight))
			.saturating_add(T::DbWeight::get().writes(11 as Weight))
	}
	// Storage: NominationPools BondedPools (r:1 w:0)
	// Storage: Staking Ledger (r:1 w:0)
	// Storage: Staking MinNominatorBond (r:1 w:0)
	// Storage: Staking Nominators (r:1 w:1)
	// Storage: Staking MaxNominatorsCount (r:1 w:0)
	// Storage: Staking Validators (r:17 w:0)
	// Storage: Staking CurrentEra (r:1 w:0)
	// Storage: Staking Bonded (r:1 w:0)
	// Storage: BagsList ListNodes (r:1 w:1)
	// Storage: BagsList ListBags (r:1 w:1)
	// Storage: BagsList CounterForListNodes (r:1 w:1)
	// Storage: Staking CounterForNominators (r:1 w:1)
	fn nominate() -> Weight {
		(79_587_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(28 as Weight))
			.saturating_add(T::DbWeight::get().writes(5 as Weight))
	}
}

// For backwards compatibility and tests
impl WeightInfo for () {
	// Storage: NominationPools MinJoinBond (r:1 w:0)
	// Storage: NominationPools Delegators (r:1 w:1)
	// Storage: NominationPools BondedPools (r:1 w:1)
	// Storage: Staking Ledger (r:1 w:1)
	// Storage: NominationPools RewardPools (r:1 w:0)
	// Storage: System Account (r:2 w:1)
	// Storage: Staking Bonded (r:1 w:0)
	// Storage: Balances Locks (r:1 w:1)
	// Storage: BagsList ListNodes (r:3 w:3)
	// Storage: BagsList ListBags (r:2 w:2)
	// Storage: NominationPools CounterForDelegators (r:1 w:1)
	fn join() -> Weight {
		(109_762_000 as Weight)
			.saturating_add(RocksDbWeight::get().reads(15 as Weight))
			.saturating_add(RocksDbWeight::get().writes(11 as Weight))
	}
	// Storage: NominationPools Delegators (r:1 w:1)
	// Storage: NominationPools BondedPools (r:1 w:0)
	// Storage: NominationPools RewardPools (r:1 w:1)
	// Storage: System Account (r:1 w:1)
	fn claim_payout() -> Weight {
		(45_848_000 as Weight)
			.saturating_add(RocksDbWeight::get().reads(4 as Weight))
			.saturating_add(RocksDbWeight::get().writes(3 as Weight))
	}
	// Storage: NominationPools Delegators (r:1 w:1)
	// Storage: NominationPools BondedPools (r:1 w:1)
	// Storage: NominationPools RewardPools (r:1 w:1)
	// Storage: System Account (r:2 w:1)
	// Storage: NominationPools SubPoolsStorage (r:1 w:1)
	// Storage: Staking CurrentEra (r:1 w:0)
	// Storage: Staking Ledger (r:1 w:1)
	// Storage: Staking Nominators (r:1 w:0)
	// Storage: Staking MinNominatorBond (r:1 w:0)
	// Storage: Balances Locks (r:1 w:1)
	// Storage: BagsList ListNodes (r:3 w:3)
	// Storage: Staking Bonded (r:1 w:0)
	// Storage: BagsList ListBags (r:2 w:2)
	// Storage: NominationPools CounterForSubPoolsStorage (r:1 w:1)
	fn unbond_other() -> Weight {
		(114_657_000 as Weight)
			.saturating_add(RocksDbWeight::get().reads(18 as Weight))
			.saturating_add(RocksDbWeight::get().writes(13 as Weight))
	}
	// Storage: Staking Ledger (r:1 w:1)
	// Storage: Staking CurrentEra (r:1 w:0)
	// Storage: Balances Locks (r:1 w:1)
	fn pool_withdraw_unbonded(s: u32, ) -> Weight {
		(33_087_000 as Weight)
			// Standard Error: 0
			.saturating_add((50_000 as Weight).saturating_mul(s as Weight))
			.saturating_add(RocksDbWeight::get().reads(3 as Weight))
			.saturating_add(RocksDbWeight::get().writes(2 as Weight))
	}
	// Storage: NominationPools Delegators (r:1 w:1)
	// Storage: Staking CurrentEra (r:1 w:0)
	// Storage: NominationPools SubPoolsStorage (r:1 w:1)
	// Storage: NominationPools BondedPools (r:1 w:0)
	// Storage: Staking Ledger (r:1 w:1)
	// Storage: Balances Locks (r:1 w:1)
	// Storage: System Account (r:1 w:1)
	// Storage: NominationPools CounterForDelegators (r:1 w:1)
	fn withdraw_unbonded_other_update(s: u32) -> Weight {
		(66_522_000 as Weight)
			// Standard Error: 0
			.saturating_add((57_000 as Weight).saturating_mul(s as Weight))
			.saturating_add(RocksDbWeight::get().reads(8 as Weight))
			.saturating_add(RocksDbWeight::get().writes(6 as Weight))
	}
	// Storage: NominationPools CounterForDelegators (r:1 w:1)
	fn withdraw_unbonded_other_kill(s: u32) -> Weight {
		(66_522_000 as Weight)
			// Standard Error: 0
			.saturating_add((57_000 as Weight).saturating_mul(s as Weight))
			.saturating_add(RocksDbWeight::get().reads(8 as Weight))
			.saturating_add(RocksDbWeight::get().writes(6 as Weight))
	}
	// Storage: Staking MinNominatorBond (r:1 w:0)
	// Storage: NominationPools MinCreateBond (r:1 w:0)
	// Storage: NominationPools MaxPools (r:1 w:0)
	// Storage: NominationPools CounterForBondedPools (r:1 w:1)
	// Storage: NominationPools Delegators (r:1 w:1)
	// Storage: System ParentHash (r:1 w:0)
	// Storage: unknown [0x3a65787472696e7369635f696e646578] (r:1 w:0)
	// Storage: NominationPools BondedPools (r:1 w:1)
	// Storage: Staking Ledger (r:1 w:1)
	// Storage: System Account (r:1 w:1)
	// Storage: Staking Bonded (r:1 w:1)
	// Storage: Staking CurrentEra (r:1 w:0)
	// Storage: Staking HistoryDepth (r:1 w:0)
	// Storage: Balances Locks (r:1 w:1)
	// Storage: NominationPools CounterForDelegators (r:1 w:1)
	// Storage: NominationPools RewardPools (r:1 w:1)
	// Storage: NominationPools CounterForRewardPools (r:1 w:1)
	// Storage: Staking Payee (r:0 w:1)
	fn create() -> Weight {
		(92_839_000 as Weight)
			.saturating_add(RocksDbWeight::get().reads(17 as Weight))
			.saturating_add(RocksDbWeight::get().writes(11 as Weight))
	}
	// Storage: NominationPools BondedPools (r:1 w:0)
	// Storage: Staking Ledger (r:1 w:0)
	// Storage: Staking MinNominatorBond (r:1 w:0)
	// Storage: Staking Nominators (r:1 w:1)
	// Storage: Staking MaxNominatorsCount (r:1 w:0)
	// Storage: Staking Validators (r:17 w:0)
	// Storage: Staking CurrentEra (r:1 w:0)
	// Storage: Staking Bonded (r:1 w:0)
	// Storage: BagsList ListNodes (r:1 w:1)
	// Storage: BagsList ListBags (r:1 w:1)
	// Storage: BagsList CounterForListNodes (r:1 w:1)
	// Storage: Staking CounterForNominators (r:1 w:1)
	fn nominate() -> Weight {
		(79_587_000 as Weight)
			.saturating_add(RocksDbWeight::get().reads(28 as Weight))
			.saturating_add(RocksDbWeight::get().writes(5 as Weight))
	}
}