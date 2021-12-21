// This file is part of Substrate.

// Copyright (C) 2021 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use super::*;
use crate::{
	self as multi_block,
	unsigned::{
		self as unsigned_pallet,
		miner::{BaseMiner, MinerError},
	},
	verifier as verifier_pallet,
};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_election_provider_support::{data_provider, ElectionDataProvider, Support};
pub use frame_support::{assert_noop, assert_ok};
use frame_support::{parameter_types, traits::Hooks, weights::Weight};
use parking_lot::RwLock;
use sp_core::{
	offchain::{
		testing::{PoolState, TestOffchainExt, TestTransactionPoolExt},
		OffchainDbExt, OffchainWorkerExt, TransactionPoolExt,
	},
	H256,
};
use sp_npos_elections::{EvaluateSupport, NposSolution};
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
	PerU16, Perbill,
};
use std::{sync::Arc, vec};

pub type Block = sp_runtime::generic::Block<Header, UncheckedExtrinsic>;
pub type Extrinsic = sp_runtime::testing::TestXt<Call, ()>;
pub type UncheckedExtrinsic = sp_runtime::generic::UncheckedExtrinsic<AccountId, Call, (), ()>;

frame_support::construct_runtime!(
	pub enum Runtime where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic
	{
		System: frame_system::{Pallet, Call, Event<T>, Config},
		Balances: pallet_balances::{Pallet, Call, Event<T>, Config<T>},
		MultiBlock: multi_block::{Pallet, Event<T>},
		VerifierPallet: verifier_pallet::{Pallet, Event<T>},
		UnsignedPallet: unsigned_pallet::{Pallet, Call, ValidateUnsigned},
	}
);

pub(crate) type Balance = u64;
pub(crate) type AccountId = u64;
pub(crate) type BlockNumber = u64;
pub(crate) type VoterIndex = u32;
pub(crate) type TargetIndex = u16;

sp_npos_elections::generate_solution_type!(
	pub struct TestNposSolution::<VoterIndex = VoterIndex, TargetIndex = TargetIndex, Accuracy = PerU16>(16)
);

impl codec::MaxEncodedLen for TestNposSolution {
	fn max_encoded_len() -> usize {
		todo!()
	}
}

impl frame_system::Config for Runtime {
	type SS58Prefix = ();
	type BaseCallFilter = frame_support::traits::Everything;
	type Origin = Origin;
	type Index = u64;
	type BlockNumber = BlockNumber;
	type Call = Call;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Header = Header;
	type Event = Event;
	type BlockHashCount = ();
	type DbWeight = ();
	type BlockLength = ();
	type BlockWeights = BlockWeights;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = pallet_balances::AccountData<Balance>;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type OnSetCode = ();
	type MaxConsumers = ConstU32<16>;
}

const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);
parameter_types! {
	pub const ExistentialDeposit: Balance = 1;
	pub BlockWeights: frame_system::limits::BlockWeights = frame_system::limits::BlockWeights
		::with_sensible_defaults(2 * frame_support::weights::constants::WEIGHT_PER_SECOND, NORMAL_DISPATCH_RATIO);
}

impl pallet_balances::Config for Runtime {
	type Balance = Balance;
	type Event = Event;
	type DustRemoval = ();
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
	type MaxLocks = ();
	type MaxReserves = ();
	type ReserveIdentifier = [u8; 8];
	type WeightInfo = ();
}

parameter_types! {
	pub static Targets: Vec<AccountId> = vec![10, 20, 30, 40];
	pub static Voters: Vec<(AccountId, VoteWeight, BoundedVec<AccountId, MaxVotesPerVoter>)> = vec![
		(1, 10, BoundedVec::<_, MaxVotesPerVoter>::try_from(vec![10, 20]).unwrap()),
		(2, 10, BoundedVec::<_, MaxVotesPerVoter>::try_from(vec![30, 40]).unwrap()),
		(3, 10, BoundedVec::<_, MaxVotesPerVoter>::try_from(vec![40]).unwrap()),
		(4, 10, BoundedVec::<_, MaxVotesPerVoter>::try_from(vec![10, 20, 40]).unwrap()),
		(5, 10, BoundedVec::<_, MaxVotesPerVoter>::try_from(vec![10, 30, 40]).unwrap()),
		(6, 10, BoundedVec::<_, MaxVotesPerVoter>::try_from(vec![20, 30, 40]).unwrap()),
		(7, 10, BoundedVec::<_, MaxVotesPerVoter>::try_from(vec![20, 30]).unwrap()),
		(8, 10, BoundedVec::<_, MaxVotesPerVoter>::try_from(vec![10]).unwrap()),
		// self votes.
		(10, 10, BoundedVec::<_, MaxVotesPerVoter>::try_from(vec![10]).unwrap()),
		(20, 20, BoundedVec::<_, MaxVotesPerVoter>::try_from(vec![20]).unwrap()),
		(30, 30, BoundedVec::<_, MaxVotesPerVoter>::try_from(vec![30]).unwrap()),
		(40, 40, BoundedVec::<_, MaxVotesPerVoter>::try_from(vec![40]).unwrap()),
	];
	pub static LastIteratedVoterIndex: Option<usize> = None;

	pub static OnChianFallback: bool = false;
	pub static DesiredTargets: u32 = 2;
	pub static SignedPhase: BlockNumber = 10;
	pub static UnsignedPhase: BlockNumber = 5;
	pub static MinerMaxIterations: u32 = 5;
	pub static MinerTxPriority: u64 = 100;
	pub static SolutionImprovementThreshold: Perbill = Perbill::zero();
	pub static OffchainRepeat: BlockNumber = 5;
	pub static MinerMaxWeight: Weight = BlockWeights::get().max_block;
	pub static MinerMaxLength: u32 = 256;
	pub static MockWeightInfo: bool = false;
	pub static MaxVotesPerVoter: u32 = <TestNposSolution as NposSolution>::LIMIT as u32;

	pub static EpochLength: u64 = 30;
	// by default we stick to 3 pages to host our 12 voters.
	pub static VoterSnapshotPerBlock: VoterIndex = 4;
	pub static TargetSnapshotPerBlock: TargetIndex = 8;
	pub static Lookahead: BlockNumber = 0;

	// we have 12 voters in the default setting, this should be enough to make sure they are not
	// trimmed accidentally in any test.
	#[derive(Encode, Decode, PartialEq, Eq, Debug, scale_info::TypeInfo, MaxEncodedLen)]
	pub static MaxBackersPerSupport: u32 = 12;
	// we have 4 targets in total and we desire `Desired` thereof, no single page can represent more
	// than the min of these two.
	#[derive(Encode, Decode, PartialEq, Eq, Debug, scale_info::TypeInfo, MaxEncodedLen)]
	pub static MaxSupportsPerPage: u32 = (Targets::get().len() as u32).min(DesiredTargets::get());
	pub static Pages: PageIndex = 3;
}

pub struct DualMockWeightInfo;
impl multi_block::weights::WeightInfo for DualMockWeightInfo {
	fn on_initialize_nothing() -> Weight {
		if MockWeightInfo::get() {
			Zero::zero()
		} else {
			<() as multi_block::weights::WeightInfo>::on_initialize_nothing()
		}
	}
	fn on_initialize_open_signed() -> Weight {
		if MockWeightInfo::get() {
			Zero::zero()
		} else {
			<() as multi_block::weights::WeightInfo>::on_initialize_open_signed()
		}
	}
	fn on_initialize_open_unsigned_with_snapshot() -> Weight {
		if MockWeightInfo::get() {
			Zero::zero()
		} else {
			<() as multi_block::weights::WeightInfo>::on_initialize_open_unsigned_with_snapshot()
		}
	}
	fn on_initialize_open_unsigned_without_snapshot() -> Weight {
		if MockWeightInfo::get() {
			Zero::zero()
		} else {
			<() as multi_block::weights::WeightInfo>::on_initialize_open_unsigned_without_snapshot()
		}
	}
	fn finalize_signed_phase_accept_solution() -> Weight {
		if MockWeightInfo::get() {
			Zero::zero()
		} else {
			<() as multi_block::weights::WeightInfo>::finalize_signed_phase_accept_solution()
		}
	}
	fn finalize_signed_phase_reject_solution() -> Weight {
		if MockWeightInfo::get() {
			Zero::zero()
		} else {
			<() as multi_block::weights::WeightInfo>::finalize_signed_phase_reject_solution()
		}
	}
	fn submit(c: u32) -> Weight {
		if MockWeightInfo::get() {
			Zero::zero()
		} else {
			<() as multi_block::weights::WeightInfo>::submit(c)
		}
	}
	fn elect_queued(v: u32, t: u32, a: u32, d: u32) -> Weight {
		if MockWeightInfo::get() {
			Zero::zero()
		} else {
			<() as multi_block::weights::WeightInfo>::elect_queued(v, t, a, d)
		}
	}
	fn submit_unsigned(v: u32, t: u32, a: u32, d: u32) -> Weight {
		if MockWeightInfo::get() {
			// 10 base
			// 5 per edge.
			(10 as Weight).saturating_add((5 as Weight).saturating_mul(a as Weight))
		} else {
			<() as multi_block::weights::WeightInfo>::submit_unsigned(v, t, a, d)
		}
	}
	fn feasibility_check(v: u32, t: u32, a: u32, d: u32) -> Weight {
		if MockWeightInfo::get() {
			// 10 base
			// 5 per edge.
			(10 as Weight).saturating_add((5 as Weight).saturating_mul(a as Weight))
		} else {
			<() as multi_block::weights::WeightInfo>::feasibility_check(v, t, a, d)
		}
	}
}

impl crate::verifier::Config for Runtime {
	type Event = Event;
	type SolutionImprovementThreshold = SolutionImprovementThreshold;
	type ForceOrigin = frame_system::EnsureRoot<AccountId>;
	type MaxBackersPerSupport = MaxBackersPerSupport;
	type MaxSupportsPerPage = MaxSupportsPerPage;
}

pub struct MockUnsignedWeightInfo;
impl crate::unsigned::WeightInfo for MockUnsignedWeightInfo {
	fn submit_unsigned(_v: u32, _t: u32, a: u32, _d: u32) -> Weight {
		a as Weight
	}
}

impl crate::unsigned::Config for Runtime {
	type OffchainRepeat = OffchainRepeat;
	type MinerMaxIterations = MinerMaxIterations;
	type MinerMaxWeight = MinerMaxWeight;
	type MinerMaxLength = MinerMaxLength;
	type MinerTxPriority = MinerTxPriority;
	type OffchainSolver =
		frame_election_provider_support::SequentialPhragmen<Self::AccountId, Perbill>;
	type WeightInfo = MockUnsignedWeightInfo;
}

impl crate::Config for Runtime {
	type Event = Event;
	type SignedPhase = SignedPhase;
	type UnsignedPhase = UnsignedPhase;
	type DataProvider = MockStaking;
	type BenchmarkingConfig = ();
	type Fallback = MockFallback;
	type TargetSnapshotPerBlock = TargetSnapshotPerBlock;
	type VoterSnapshotPerBlock = VoterSnapshotPerBlock;
	type Lookahead = Lookahead;
	type Solution = TestNposSolution;
	type WeightInfo = DualMockWeightInfo;
	type Verifier = VerifierPallet;
	type Pages = Pages;
}

impl<LocalCall> frame_system::offchain::SendTransactionTypes<LocalCall> for Runtime
where
	Call: From<LocalCall>,
{
	type OverarchingCall = Call;
	type Extrinsic = Extrinsic;
}

impl onchain::Config for Runtime {
	type Accuracy = sp_runtime::Perbill;
	type DataProvider = MockStaking;
	type TargetsPageSize = ();
	type VoterPageSize = ();
	type MaxBackersPerSupport = MaxBackersPerSupport;
	type MaxSupportsPerPage = MaxSupportsPerPage;
}

pub struct MockFallback;
impl ElectionProvider for MockFallback {
	type AccountId = AccountId;
	type BlockNumber = u64;
	type Error = &'static str;
	type DataProvider = MockStaking;
	type Pages = ConstU32<1>;
	type MaxBackersPerSupport = MaxBackersPerSupport;
	type MaxSupportsPerPage = MaxSupportsPerPage;

	fn elect(remaining: PageIndex) -> Result<BoundedSupportsOf<Self>, Self::Error> {
		if OnChianFallback::get() {
			onchain::OnChainSequentialPhragmen::<Runtime>::elect(remaining)
				.map_err(|_| "OnChainSequentialPhragmen failed")
		} else {
			// NOTE: this pesky little trick here is to avoid a clash of type, since `Ok` of our
			// election provider and our fallback is not the same
			let err = InitiateEmergencyPhase::<Runtime>::elect(remaining).unwrap_err();
			Err(err)
		}
	}
}

pub struct MockStaking;
impl ElectionDataProvider for MockStaking {
	type AccountId = AccountId;
	type BlockNumber = u64;
	type MaxVotesPerVoter = MaxVotesPerVoter;

	fn targets(
		maybe_max_len: Option<usize>,
		remaining: PageIndex,
	) -> data_provider::Result<Vec<AccountId>> {
		let targets = Targets::get();

		if remaining != 0 {
			return Err("targets shall not have more than a single page")
		}
		if maybe_max_len.map_or(false, |max_len| targets.len() > max_len) {
			return Err("Targets too big")
		}

		Ok(targets)
	}

	fn voters(
		maybe_max_len: Option<usize>,
		remaining: PageIndex,
	) -> data_provider::Result<
		Vec<(AccountId, VoteWeight, BoundedVec<AccountId, Self::MaxVotesPerVoter>)>,
	> {
		let mut voters = Voters::get();

		// jump to the first non-iterated, if this is a follow up.
		if let Some(index) = LastIteratedVoterIndex::get() {
			voters = voters.iter().skip(index).cloned().collect::<Vec<_>>();
		}

		// take as many as you can.
		if let Some(max_len) = maybe_max_len {
			voters.truncate(max_len)
		}

		if voters.is_empty() {
			return Ok(vec![])
		}

		if remaining > 0 {
			let last = voters.last().cloned().unwrap();
			LastIteratedVoterIndex::set(Some(
				Voters::get().iter().position(|v| v == &last).map(|i| i + 1).unwrap(),
			));
		} else {
			LastIteratedVoterIndex::set(None)
		}

		Ok(voters)
	}

	fn desired_targets() -> data_provider::Result<u32> {
		Ok(DesiredTargets::get())
	}

	fn next_election_prediction(now: u64) -> u64 {
		now + EpochLength::get() - now % EpochLength::get()
	}

	#[cfg(any(feature = "runtime-benchmarks", test))]
	fn put_snapshot(
		voters: Vec<(AccountId, VoteWeight, BoundedVec<AccountId, MaxVotesPerVoter>)>,
		targets: Vec<AccountId>,
		_target_stake: Option<VoteWeight>,
	) {
		Targets::set(targets);
		Voters::set(voters);
	}

	#[cfg(any(feature = "runtime-benchmarks", test))]
	fn clear() {
		Targets::set(vec![]);
		Voters::set(vec![]);
	}

	#[cfg(any(feature = "runtime-benchmarks", test))]
	fn add_voter(
		voter: AccountId,
		weight: VoteWeight,
		targets: BoundedVec<AccountId, MaxVotesPerVoter>,
	) {
		let mut current = Voters::get();
		current.push((voter, weight, targets));
		Voters::set(current);
	}

	#[cfg(any(feature = "runtime-benchmarks", test))]
	fn add_target(target: AccountId) {
		let mut current = Targets::get();
		current.push(target);
		Targets::set(current);

		// to be on-par with staking, we add a self vote as well. the stake is really not that
		// important.
		let mut current = Voters::get();
		current.push((target, ExistentialDeposit::get() as u64, vec![target].try_into().unwrap()));
		Voters::set(current);
	}
}

#[derive(Default)]
pub struct ExtBuilder {}

impl ExtBuilder {
	pub(crate) fn max_backing_per_target(self, c: u32) -> Self {
		<MaxBackersPerSupport>::set(c);
		self
	}
	pub(crate) fn miner_tx_priority(self, p: u64) -> Self {
		<MinerTxPriority>::set(p);
		self
	}
	pub(crate) fn solution_improvement_threshold(self, p: Perbill) -> Self {
		<SolutionImprovementThreshold>::set(p);
		self
	}
	pub(crate) fn phases(self, signed: u64, unsigned: u64) -> Self {
		<SignedPhase>::set(signed);
		<UnsignedPhase>::set(unsigned);
		self
	}
	pub(crate) fn pages(self, pages: PageIndex) -> Self {
		<Pages>::set(pages);
		self
	}
	pub(crate) fn voter_per_page(self, count: u32) -> Self {
		<VoterSnapshotPerBlock>::set(count);
		self
	}
	pub(crate) fn miner_weight(self, weight: Weight) -> Self {
		<MinerMaxWeight>::set(weight);
		self
	}
	pub(crate) fn miner_length(self, len: u32) -> Self {
		<MinerMaxLength>::set(len);
		self
	}
	pub(crate) fn desired_targets(self, t: u32) -> Self {
		<DesiredTargets>::set(t);
		self
	}
	pub(crate) fn add_voter(self, who: AccountId, stake: Balance, targets: Vec<AccountId>) -> Self {
		VOTERS.with(|v| v.borrow_mut().push((who, stake, targets.try_into().unwrap())));
		self
	}
	pub(crate) fn onchain_fallback(self, enable: bool) -> Self {
		OnChianFallback::set(enable);
		self
	}
	pub(crate) fn build_unchecked(self) -> sp_io::TestExternalities {
		sp_tracing::try_init_simple();
		let mut storage =
			frame_system::GenesisConfig::default().build_storage::<Runtime>().unwrap();

		let _ = pallet_balances::GenesisConfig::<Runtime> {
			balances: vec![
				// bunch of account for submitting stuff only.
				(99, 100),
				(999, 100),
				(9999, 100),
			],
		}
		.assimilate_storage(&mut storage);

		sp_io::TestExternalities::from(storage)
	}

	/// Warning: this does not execute the post-sanity-checks.
	pub(crate) fn build_offchainify(
		self,
		iters: u32,
	) -> (sp_io::TestExternalities, Arc<RwLock<PoolState>>) {
		let mut ext = self.build_unchecked();
		let (offchain, offchain_state) = TestOffchainExt::new();
		let (pool, pool_state) = TestTransactionPoolExt::new();

		let mut seed = [0_u8; 32];
		seed[0..4].copy_from_slice(&iters.to_le_bytes());
		offchain_state.write().seed = seed;

		ext.register_extension(OffchainDbExt::new(offchain.clone()));
		ext.register_extension(OffchainWorkerExt::new(offchain));
		ext.register_extension(TransactionPoolExt::new(pool));

		(ext, pool_state)
	}

	/// Build the externalities, and execute the given  s`test` closure with it.
	pub(crate) fn build_and_execute(self, test: impl FnOnce() -> ()) {
		let mut ext = self.build_unchecked();
		ext.execute_with_sanity_checks(test);
	}
}

pub trait ExecuteWithSanityChecks {
	fn execute_with_sanity_checks(&mut self, test: impl FnOnce() -> ());
}

impl ExecuteWithSanityChecks for sp_io::TestExternalities {
	fn execute_with_sanity_checks(&mut self, test: impl FnOnce() -> ()) {
		self.execute_with(test);
		self.execute_with(sanity_checks)
	}
}

fn sanity_checks() {
	let _ = VerifierPallet::sanity_check().unwrap();
	let _ = UnsignedPallet::sanity_check().unwrap();
	let _ = MultiBlock::sanity_check().unwrap();
}

pub fn balances(who: &u64) -> (u64, u64) {
	(Balances::free_balance(who), Balances::reserved_balance(who))
}

pub fn witness() -> SolutionOrSnapshotSize {
	let voters = Snapshot::<Runtime>::voters_iter_flattened().count() as u32;
	let targets = Snapshot::<Runtime>::targets().map(|t| t.len() as u32).unwrap_or_default();
	SolutionOrSnapshotSize { voters, targets }
}

/// Fully verify a solution.
///
/// This will progress the blocks until the verifier pallet is done verifying it.
///
/// The solution must have already been loaded via `load_solution_for_verification`.
///
/// Return the final supports, which is the outcome. If this succeeds, then the valid variant of the
/// `QueuedSolution` form `verifier` is ready to be read.
pub fn roll_to_full_verification() -> Vec<BoundedSupportsOf<MultiBlock>> {
	// we must be ready to verify.
	assert_eq!(VerifierPallet::status(), Some(Pages::get() - 1));

	while VerifierPallet::status().is_some() {
		roll_to(System::block_number() + 1);
	}

	(MultiBlock::lsp()..=MultiBlock::msp())
		.map(|p| VerifierPallet::get_queued_solution_page(p).unwrap_or_default())
		.collect::<Vec<_>>()
}

/// Load a full raw paged solution for verification.
pub fn load_solution_for_verification(raw_paged: PagedRawSolution<Runtime>) {
	// set
	for (page_index, solution_page) in raw_paged.solution_pages.pagify(Pages::get()) {
		assert_ok!(
			VerifierPallet::set_unverified_solution_page(page_index, solution_page.clone(),)
		);
	}
	// and seal the ok solution against the verifier
	assert_ok!(VerifierPallet::seal_unverified_solution(raw_paged.score));
}

/// Generate a single page of `TestNposSolution` from the give supports.
///
/// All of the voters in this support must live in a single page of the snapshot, noted by
/// `snapshot_page`.
pub fn solution_from_supports(
	supports: sp_npos_elections::Supports<AccountId>,
	snapshot_page: PageIndex,
) -> TestNposSolution {
	let staked = sp_npos_elections::supports_to_staked_assignment(supports);
	let assignments = sp_npos_elections::assignment_staked_to_ratio_normalized(staked).unwrap();

	let voters = crate::Snapshot::<Runtime>::voters(snapshot_page).unwrap();
	let targets = crate::Snapshot::<Runtime>::targets().unwrap();
	let voter_index = helpers::voter_index_fn_linear::<Runtime>(&voters);
	let target_index = helpers::target_index_fn_linear::<Runtime>(&targets);

	TestNposSolution::from_assignment(&assignments, &voter_index, &target_index).unwrap()
}

/// Generate a raw paged solution from the given vector of supports.
///
/// Given vector must be aligned with the snapshot, at most need to be 'pagified' which we do
/// internally.
pub fn raw_paged_from_supports(
	paged_supports: Vec<sp_npos_elections::Supports<AccountId>>,
	round: u32,
) -> PagedRawSolution<Runtime> {
	let score = {
		let flattened = paged_supports.iter().cloned().flatten().collect::<Vec<_>>();
		flattened.evaluate()
	};

	let solution_pages = paged_supports
		.pagify(Pages::get())
		.map(|(page_index, page_support)| solution_from_supports(page_support.to_vec(), page_index))
		.collect::<Vec<_>>();

	let solution_pages = solution_pages.try_into().unwrap();
	PagedRawSolution { solution_pages, score, round }
}

/// ensure that the snapshot fully exists.
pub fn ensure_full_snapshot() {
	Snapshot::<Runtime>::assert_snapshot(true, Pages::get())
}

/// Simple wrapper for mining a new solution. Just more handy in case the interface of mine solution
/// changes.
///
/// For testing, we never want to do reduce.
pub fn mine_full_solution() -> Result<PagedRawSolution<Runtime>, MinerError<Runtime>> {
	BaseMiner::<Runtime>::mine_solution(Pages::get(), false)
}

/// Same as [`mine_full_solution`] but with custom pages.
pub fn mine_solution(pages: PageIndex) -> Result<PagedRawSolution<Runtime>, MinerError<Runtime>> {
	BaseMiner::<Runtime>::mine_solution(pages, false)
}

/// Assert that `count` voters exist across `pages` number of pages.
pub fn ensure_voters(pages: PageIndex, count: usize) {
	assert_eq!(crate::Snapshot::<Runtime>::voter_pages(), pages);
	assert_eq!(crate::Snapshot::<Runtime>::voters_iter_flattened().count(), count);
}

/// Assert that `count` targets exist across `pages` number of pages.
pub fn ensure_targets(pages: PageIndex, count: usize) {
	assert_eq!(crate::Snapshot::<Runtime>::target_pages(), pages);
	assert_eq!(crate::Snapshot::<Runtime>::targets().unwrap().len(), count);
}

/// get the events of the multi-block pallet.
pub fn multi_block_events() -> Vec<crate::Event<Runtime>> {
	System::events()
		.into_iter()
		.map(|r| r.event)
		.filter_map(|e| if let Event::MultiBlock(inner) = e { Some(inner) } else { None })
		.collect::<Vec<_>>()
}

/// get the events of the verifier pallet.
pub fn verifier_events() -> Vec<crate::verifier::Event<Runtime>> {
	System::events()
		.into_iter()
		.map(|r| r.event)
		.filter_map(|e| if let Event::VerifierPallet(inner) = e { Some(inner) } else { None })
		.collect::<Vec<_>>()
}

/// proceed block number to `n`.
pub fn roll_to(n: BlockNumber) {
	let now = System::block_number();
	for i in now + 1..=n {
		System::set_block_number(i);
		MultiBlock::on_initialize(i);
		VerifierPallet::on_initialize(i);
		UnsignedPallet::on_initialize(i);
	}
}

/// proceed block number to whenever the snapshot is fully created (`Phase::Snapshot(0)`).
pub fn roll_to_snapshot_created() {
	let mut now = System::block_number() + 1;
	while !matches!(MultiBlock::current_phase(), Phase::Snapshot(0)) {
		System::set_block_number(now);
		MultiBlock::on_initialize(now);
		VerifierPallet::on_initialize(now);
		UnsignedPallet::on_initialize(now);
		now += 1;
	}
}

/// proceed block number to whenever the unsigned phase is open (`Phase::Unsigned(_)`).
pub fn roll_to_unsigned_open() {
	let mut now = System::block_number() + 1;
	while !matches!(MultiBlock::current_phase(), Phase::Unsigned(_)) {
		System::set_block_number(now);
		MultiBlock::on_initialize(now);
		VerifierPallet::on_initialize(now);
		UnsignedPallet::on_initialize(now);
		now += 1;
	}
}

/// proceed block number to `n`, while running all offchain workers as well.
pub fn roll_to_with_ocw(n: BlockNumber, maybe_pool: Option<Arc<RwLock<PoolState>>>) {
	use sp_runtime::traits::Dispatchable;
	let now = System::block_number();
	for i in now + 1..=n {
		// check the offchain transaction pool, and if anything's there, submit it.
		if let Some(ref pool) = maybe_pool {
			pool.read()
				.transactions
				.clone()
				.into_iter()
				.map(|uxt| <Extrinsic as codec::Decode>::decode(&mut &*uxt).unwrap())
				.for_each(|xt| {
					xt.call.dispatch(frame_system::RawOrigin::None.into()).unwrap();
				});
			pool.try_write().unwrap().transactions.clear();
		}
		System::set_block_number(i);
		MultiBlock::on_initialize(i);
		VerifierPallet::on_initialize(i);
		UnsignedPallet::on_initialize(i);

		MultiBlock::offchain_worker(i);
		VerifierPallet::offchain_worker(i);
		UnsignedPallet::offchain_worker(i);
	}
}

/// An invalid solution with any score.
pub fn fake_unsigned_solution(score: ElectionScore) -> PagedRawSolution<Runtime> {
	PagedRawSolution {
		score,
		solution_pages: vec![Default::default()].try_into().unwrap(),
		..Default::default()
	}
}

/// A real solution that's valid, but has a really bad score.
///
/// This is different from `solution_from_supports` in that it does not require the snapshot to
/// exist.
pub fn raw_paged_solution_low_score() -> PagedRawSolution<Runtime> {
	PagedRawSolution {
		solution_pages: vec![TestNposSolution {
			// 2 targets, both voting for themselves
			votes1: vec![(0, 0), (1, 2)],
			..Default::default()
		}]
		.try_into()
		.unwrap(),
		round: 1,
		score: [
			10,  // lowest staked
			20,  // total staked
			200, // sum of stakes squared
		],
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn targets() {
		ExtBuilder::default().build_and_execute(|| {
			assert_eq!(Targets::get().len(), 4);

			// any non-zero page is error
			assert!(MockStaking::targets(None, 1).is_err());
			assert!(MockStaking::targets(None, 2).is_err());

			// but 0 is fine.
			assert_eq!(MockStaking::targets(None, 0).unwrap().len(), 4);

			// fetch less targets is error.
			assert!(MockStaking::targets(Some(2), 0).is_err());

			// more targets is fine.
			assert!(MockStaking::targets(Some(4), 0).is_ok());
			assert!(MockStaking::targets(Some(5), 0).is_ok());
		});
	}

	#[test]
	fn multi_page_votes() {
		ExtBuilder::default().build_and_execute(|| {
			assert_eq!(MockStaking::voters(None, 0).unwrap().len(), 12);
			assert!(LastIteratedVoterIndex::get().is_none());

			assert_eq!(
				MockStaking::voters(Some(4), 0)
					.unwrap()
					.into_iter()
					.map(|(x, _, _)| x)
					.collect::<Vec<_>>(),
				vec![1, 2, 3, 4],
			);
			assert!(LastIteratedVoterIndex::get().is_none());

			assert_eq!(
				MockStaking::voters(Some(4), 2)
					.unwrap()
					.into_iter()
					.map(|(x, _, _)| x)
					.collect::<Vec<_>>(),
				vec![1, 2, 3, 4],
			);
			assert_eq!(LastIteratedVoterIndex::get().unwrap(), 4);

			assert_eq!(
				MockStaking::voters(Some(4), 1)
					.unwrap()
					.into_iter()
					.map(|(x, _, _)| x)
					.collect::<Vec<_>>(),
				vec![5, 6, 7, 8],
			);
			assert_eq!(LastIteratedVoterIndex::get().unwrap(), 8);

			assert_eq!(
				MockStaking::voters(Some(4), 0)
					.unwrap()
					.into_iter()
					.map(|(x, _, _)| x)
					.collect::<Vec<_>>(),
				vec![10, 20, 30, 40],
			);
			assert!(LastIteratedVoterIndex::get().is_none());
		})
	}
}
