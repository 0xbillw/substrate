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

// TODO: clean and standardize the imports

use crate::{helpers, SolutionOf, SupportsOf};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_election_provider_support::{ExtendedBalance, PageIndex};
use sp_npos_elections::{ElectionScore, NposSolution};
use sp_std::{collections::btree_map::BTreeMap, prelude::*};

use super::*;
use frame_support::{dispatch::Weight, ensure, traits::Get, RuntimeDebug};

use pallet::*;

#[derive(Encode, Decode, scale_info::TypeInfo, Clone, Copy, MaxEncodedLen, RuntimeDebug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub enum Status {
	Ongoing(PageIndex),
	Nothing,
}

impl Default for Status {
	fn default() -> Self {
		Self::Nothing
	}
}

#[derive(Encode, Decode, scale_info::TypeInfo, Clone, Copy, MaxEncodedLen)]
enum ValidSolution {
	X,
	Y,
}

impl Default for ValidSolution {
	fn default() -> Self {
		ValidSolution::Y
	}
}

impl ValidSolution {
	fn other(&self) -> Self {
		match *self {
			ValidSolution::X => ValidSolution::Y,
			ValidSolution::Y => ValidSolution::X,
		}
	}
}

#[frame_support::pallet]
pub(crate) mod pallet {
	use crate::{
		types::{Pagify, SupportsOf},
		verifier::Verifier,
	};

	use super::*;
	use frame_support::pallet_prelude::{ValueQuery, *};
	use frame_system::pallet_prelude::*;
	use sp_npos_elections::evaluate_support_core;

	/// A simple newtype that represents the partial backing of a winner. It only stores the total
	/// backing, and the sum of backings, as opposed to a [`sp_npos_elections::Support`] that also
	/// stores all of the backers' individual contribution.
	///
	/// This is mainly here to allow us to implement `Backings` for it.
	#[derive(Default, Encode, Decode, MaxEncodedLen, TypeInfo)]
	pub struct PartialBackings {
		/// The total backing of this particular winner.
		pub total: ExtendedBalance,
		/// The number of backers.
		pub backers: u32,
	}

	impl sp_npos_elections::Backings for PartialBackings {
		fn total(&self) -> ExtendedBalance {
			self.total
		}
	}

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: crate::Config {
		/// The overarching event type.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// Origin that can control this pallet. Note that any action taken by this origin (such)
		/// as providing an emergency solution is not checked. Thus, it must be a trusted origin.
		type ForceOrigin: EnsureOrigin<Self::Origin>;

		/// The minimum amount of improvement to the solution score that defines a solution as
		/// "better".
		#[pallet::constant]
		type SolutionImprovementThreshold: Get<sp_runtime::Perbill>;

		/// Maximum number of voters that can support a single target, among ALL pages of a
		/// verifying solution. It can only ever be checked on the last page of any given
		/// verification.
		///
		/// This must be set such that the memory limits in the rest of the system are well
		/// respected.
		type MaxBackersPerWinner: Get<u32> + TypeInfo + MaxEncodedLen + sp_std::fmt::Debug;

		/// Maximum number of supports (aka. winners/validators/targets) that can be represented in
		/// a page of results.
		type MaxWinnersPerPage: Get<u32> + TypeInfo + MaxEncodedLen + sp_std::fmt::Debug;

		type SolutionDataProvider: crate::verifier::SolutionDataProvider<Solution = Self::Solution>;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The given call is not allowed at this time.
		CallNotAllowed,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T> {
		/// A verification failed at the given page.
		///
		/// NOTE: if the index is 0, then this could mean either the feasibility of the last page
		/// was wrong, or the final checks of `finalize_verification` failed.
		VerificationFailed(PageIndex, FeasibilityError),
		/// The given page of a solution has been verified, with the given number of winners being
		/// found in it.
		Verified(PageIndex, u32),
		/// A solution with the given score has replaced our current best solution.
		Queued(ElectionScore, Option<ElectionScore>),
	}

	// ---- All storage items about the verifying solution.
	/// A wrapper interface for the storage items related to the queued solution.
	pub(crate) struct QueuedSolution<T: Config>(sp_std::marker::PhantomData<T>);
	impl<T: Config> QueuedSolution<T> {
		/// Return the `score` and `winner_count` of verifying solution.
		///
		/// Assumes that all the corresponding pages of `QueuedSolutionBackings` exist, then it
		/// computes the final score of the solution that is currently at the end of its
		/// verification process.
		///
		/// This solution corresponds to whatever is stored in the INVALID variant of
		/// `QueuedSolution`. Recall that the score of this solution is not yet verified, so it
		/// should never become `valid`.
		pub(crate) fn final_score() -> Result<(ElectionScore, u32), FeasibilityError> {
			// ensure that this is only called when all pages are verified individually.
			if QueuedSolutionBackings::<T>::iter_keys().count() != T::Pages::get() as usize {
				return Err(FeasibilityError::Incomplete)
			}

			let mut total_supports: BTreeMap<T::AccountId, PartialBackings> = Default::default();
			// ASSUMPTION: in the staking level, we will eventually collect all exposures, which
			// has the same length as the `total_supports`, but it even has more byte size to it.
			// Thus, this code is 100% safe, but on the staking side there should be caution to
			// make sure exposure collection cannot fail.
			for (who, PartialBackings { backers, total }) in
				QueuedSolutionBackings::<T>::iter().map(|(_, pb)| pb).flatten()
			{
				let mut entry = total_supports.entry(who).or_default();
				entry.total = entry.total.saturating_add(total);
				entry.backers = entry.backers.saturating_add(backers);

				if entry.backers > T::MaxBackersPerWinner::get() {
					return Err(FeasibilityError::TooManyBackings)
				}
			}

			let winner_count = total_supports.len() as u32;
			let score = evaluate_support_core(total_supports.into_iter().map(|(_who, pb)| pb));

			Ok((score, winner_count))
		}

		/// Finalize a correct solution.
		///
		/// Should be called at the end of a verification process, once we are sure that a certain
		/// solution is 100% correct. It stores its score, flips the pointer to it being the current
		/// best one, and clears all the backings.
		///
		/// NOTE: we don't check if this is a better score, the call site must ensure that.
		pub(crate) fn finalize_correct(score: ElectionScore) {
			sublog!(
				info,
				"verifier",
				"finalizing verification a correct solution, replacing old score {:?} with {:?}",
				QueuedSolutionScore::<T>::get(),
				score
			);

			QueuedValidVariant::<T>::mutate(|v| *v = v.other());
			QueuedSolutionScore::<T>::put(score);

			// TODO: THIS IS CRITICAL AT THIS POINT. otherwise reducing T::Pages could fuck us real
			// hard, real real hard. Write a test for this.
			QueuedSolutionBackings::<T>::remove_all(None);
			// Clear what was previously the valid variant.
			Self::clear_invalid();
		}

		/// Clear all relevant information of an invalid solution.
		///
		/// Should be called at any step, if we encounter an issue which makes the solution
		/// infeasible.
		pub(crate) fn clear_invalid() {
			match Self::invalid() {
				ValidSolution::X => QueuedSolutionX::<T>::remove_all(None),
				ValidSolution::Y => QueuedSolutionY::<T>::remove_all(None),
			};
			QueuedSolutionBackings::<T>::remove_all(None);
			// NOTE: we don't flip the variant, this is still the empty slot.
		}

		/// Clear all relevant information of the valid solution.
		///
		/// This should only be used when we intend to replace the valid solution with something
		/// else (either better, or when being forced).
		pub(crate) fn clear_valid() {
			match Self::valid() {
				ValidSolution::X => QueuedSolutionX::<T>::remove_all(None),
				ValidSolution::Y => QueuedSolutionY::<T>::remove_all(None),
			};
			QueuedSolutionScore::<T>::kill();
		}

		/// Write a single page of a valid solution into the `invalid` variant of the storage.
		///
		/// This should only be called once we are sure that this particular page is 100% correct.
		///
		/// This is called after *a page* has been validated, but the entire solution is not yet
		/// known to be valid. At this stage, we write to the invalid variant. Once all pages are
		/// verified, a call to [`finalize_correct`] will seal the correct pages and flip the
		/// invalid/valid variants.
		pub(crate) fn set_invalid_page(page: PageIndex, supports: SupportsOf<Pallet<T>>) {
			use frame_support::traits::TryCollect;
			let backings: BoundedVec<_, _> = supports
				.iter()
				.map(|(x, s)| (x.clone(), PartialBackings { total: s.total, backers: s.voters.len() as u32 } ))
				.try_collect()
				.expect("`SupportsOf` is bounded by <Pallet<T> as Verifier>::MaxWinnersPerPage, which is assured to be the same as `T::MaxWinnersPerPage` in an integrity test");
			QueuedSolutionBackings::<T>::insert(page, backings);

			match Self::invalid() {
				ValidSolution::X => QueuedSolutionX::<T>::insert(page, supports),
				ValidSolution::Y => QueuedSolutionY::<T>::insert(page, supports),
			}
		}

		/// Forcibly set a valid solution.
		///
		/// Writes all the given pages, and the provided score blindly.
		pub(crate) fn force_set_valid(
			paged_supports: BoundedVec<SupportsOf<Pallet<T>>, T::Pages>,
			score: ElectionScore,
		) {
			for (page_index, supports) in paged_supports.pagify(T::Pages::get()) {
				match Self::valid() {
					ValidSolution::X => QueuedSolutionX::<T>::insert(page_index, supports),
					ValidSolution::Y => QueuedSolutionY::<T>::insert(page_index, supports),
				}
			}
			QueuedSolutionScore::<T>::put(score);
		}

		/// Write a single page to the valid variant directly.
		///
		/// This is not the normal flow of writing, and the solution is not checked.
		///
		/// This is only useful to override the valid solution with a single (likely backup)
		/// solution.
		pub(crate) fn force_set_single_page_valid(
			page: PageIndex,
			supports: SupportsOf<Pallet<T>>,
			score: ElectionScore,
		) {
			// clear everything about valid solutions.
			Self::clear_valid();

			// write a single new page.
			match Self::valid() {
				ValidSolution::X => QueuedSolutionX::<T>::insert(page, supports),
				ValidSolution::Y => QueuedSolutionY::<T>::insert(page, supports),
			}

			// write the score.
			QueuedSolutionScore::<T>::put(score);
		}

		/// Clear all storage items.
		///
		/// Should only be called once everything is done.
		pub(crate) fn kill() {
			QueuedSolutionX::<T>::remove_all(None);
			QueuedSolutionY::<T>::remove_all(None);
			QueuedValidVariant::<T>::kill();
			QueuedSolutionBackings::<T>::remove_all(None);
			QueuedSolutionScore::<T>::kill();
		}

		/// The score of the current best solution, if any.
		pub(crate) fn queued_solution() -> Option<ElectionScore> {
			QueuedSolutionScore::<T>::get()
		}

		/// Get a page of the current queued (aka valid) solution.
		pub(crate) fn get_queued_solution_page(page: PageIndex) -> Option<SupportsOf<Pallet<T>>> {
			match Self::valid() {
				ValidSolution::X => QueuedSolutionX::<T>::get(page),
				ValidSolution::Y => QueuedSolutionY::<T>::get(page),
			}
		}

		#[cfg(test)]
		pub(crate) fn valid_iter() -> impl Iterator<Item = (PageIndex, SupportsOf<Pallet<T>>)> {
			match Self::valid() {
				ValidSolution::X => QueuedSolutionX::<T>::iter(),
				ValidSolution::Y => QueuedSolutionY::<T>::iter(),
			}
		}

		#[cfg(test)]
		pub(crate) fn invalid_iter() -> impl Iterator<Item = (PageIndex, SupportsOf<Pallet<T>>)> {
			match Self::invalid() {
				ValidSolution::X => QueuedSolutionX::<T>::iter(),
				ValidSolution::Y => QueuedSolutionY::<T>::iter(),
			}
		}

		#[cfg(test)]
		pub(crate) fn get_invalid_page(page: PageIndex) -> Option<SupportsOf<Pallet<T>>> {
			match Self::invalid() {
				ValidSolution::X => QueuedSolutionX::<T>::get(page),
				ValidSolution::Y => QueuedSolutionY::<T>::get(page),
			}
		}

		#[cfg(test)]
		pub(crate) fn get_valid_page(page: PageIndex) -> Option<SupportsOf<Pallet<T>>> {
			match Self::valid() {
				ValidSolution::X => QueuedSolutionX::<T>::get(page),
				ValidSolution::Y => QueuedSolutionY::<T>::get(page),
			}
		}

		#[cfg(test)]
		pub(crate) fn get_backing_page(
			page: PageIndex,
		) -> Option<BoundedVec<(T::AccountId, PartialBackings), T::MaxWinnersPerPage>> {
			QueuedSolutionBackings::<T>::get(page)
		}

		#[cfg(test)]
		pub(crate) fn backing_iter() -> impl Iterator<
			Item = (PageIndex, BoundedVec<(T::AccountId, PartialBackings), T::MaxWinnersPerPage>),
		> {
			QueuedSolutionBackings::<T>::iter()
		}

		fn valid() -> ValidSolution {
			QueuedValidVariant::<T>::get()
		}

		fn invalid() -> ValidSolution {
			QueuedValidVariant::<T>::get().other()
		}
	}

	// Begin storage items wrapped by QueuedSolution.

	/// The `X` variant of the current queued solution. Might be the valid one or not.
	///
	/// The two variants of this storage item is to avoid the need of copying. Recall that once a
	/// `VerifyingSolution` is being processed, it needs to write its partial supports *somewhere*.
	/// Writing theses supports on top of a *good* queued supports is wrong, since we might bail.
	/// Writing them to a bugger and copying at the ned is slightly better, but expensive. This flag
	/// system is best of both worlds.
	#[pallet::storage]
	type QueuedSolutionX<T: Config> = StorageMap<_, Twox64Concat, PageIndex, SupportsOf<Pallet<T>>>;
	#[pallet::storage]
	/// The `Y` variant of the current queued solution. Might be the valid one or not.
	type QueuedSolutionY<T: Config> = StorageMap<_, Twox64Concat, PageIndex, SupportsOf<Pallet<T>>>;
	/// Pointer to the variant of [`QueuedSolutionX`] or [`QueuedSolutionY`] that is currently
	/// valid.
	#[pallet::storage]
	type QueuedValidVariant<T: Config> = StorageValue<_, ValidSolution, ValueQuery>;
	/// The `(amount, count)` of backings, divided per page.
	///
	/// This is stored because in the last block of verification we need them to compute the score,
	/// and check `MaxBackersPerWinner`.
	///
	/// This can only ever live for the invalid variant of the solution. Once it is valid, we don't
	/// need this information anymore; the score is already computed once in
	/// [`QueuedSolutionScore`], and the backing counts are checked.
	#[pallet::storage]
	type QueuedSolutionBackings<T: Config> = StorageMap<
		_,
		Twox64Concat,
		PageIndex,
		BoundedVec<(T::AccountId, PartialBackings), T::MaxWinnersPerPage>,
	>;

	/// The score of the valid variant of [`QueuedSolution`].
	///
	/// This only ever lives for the `valid` variant.
	#[pallet::storage]
	type QueuedSolutionScore<T: Config> = StorageValue<_, ElectionScore>;

	// --- End of storage items wrapped by QueuedSolution.

	/// The minimum score that each 'untrusted' solution must attain in order to be considered
	/// feasible.
	///
	/// Can be set via `set_minimum_untrusted_score`.
	#[pallet::storage]
	#[pallet::getter(fn minimum_untrusted_score)]
	pub(crate) type MinimumUntrustedScore<T: Config> = StorageValue<_, ElectionScore>;

	#[pallet::storage]
	#[pallet::getter(fn status_storage)]
	pub(crate) type StatusStorage<T: Config> = StorageValue<_, Status, ValueQuery>;

	#[pallet::pallet]
	#[pallet::generate_storage_info]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Set a new value for `MinimumUntrustedScore`.
		///
		/// Dispatch origin must be aligned with `T::ForceOrigin`.
		///
		/// This check can be turned off by setting the value to `None`.
		#[pallet::weight(T::DbWeight::get().writes(1))]
		pub fn set_minimum_untrusted_score(
			origin: OriginFor<T>,
			maybe_next_score: Option<ElectionScore>,
		) -> DispatchResult {
			T::ForceOrigin::ensure_origin(origin)?;
			<MinimumUntrustedScore<T>>::set(maybe_next_score);
			Ok(())
		}

		/// Set a solution in the queue, to be handed out to the client of this pallet in the next
		/// call to [`Verifier::get_queued_solution_page`].
		///
		/// This can only be set by `T::ForceOrigin`, and only when the phase is `Emergency`.
		///
		/// The solution is not checked for any feasibility and is assumed to be trustworthy, as any
		/// feasibility check itself can in principle cause the election process to fail (due to
		/// memory/weight constrains).
		#[pallet::weight(T::DbWeight::get().reads_writes(1, 1))]
		pub fn set_emergency_solution(
			origin: OriginFor<T>,
			paged_supports: Vec<sp_npos_elections::Supports<T::AccountId>>,
			claimed_score: ElectionScore,
		) -> DispatchResult {
			T::ForceOrigin::ensure_origin(origin)?;

			ensure!(crate::Pallet::<T>::current_phase().is_emergency(), Error::<T>::CallNotAllowed);

			use frame_election_provider_support::TryIntoBoundedSupports;
			let bounded_supports = paged_supports
				.into_iter()
				.map(|s| s.try_into_bounded_supports())
				.collect::<Result<Vec<_>, _>>()
				.map_err::<DispatchError, _>(|_| "wrong support others".into())? // TODO: test
				.try_into()
				.map_err(|_| <crate::Error<T>>::WrongPageCount)?; // TODO: test

			QueuedSolution::<T>::force_set_valid(bounded_supports, claimed_score);

			Ok(())
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<T::BlockNumber> for Pallet<T> {
		fn integrity_test() {
			// ensure that we have funneled some of our type parameters EXACTLY as-is to the
			// verifier pallet.
			assert_eq!(T::MaxWinnersPerPage::get(), <Self as Verifier>::MaxWinnersPerPage::get());
			assert_eq!(
				T::MaxBackersPerWinner::get(),
				<Self as Verifier>::MaxBackersPerWinner::get()
			);
		}

		fn on_initialize(_n: T::BlockNumber) -> Weight {
			Self::do_on_initialize()
		}
	}
}

impl<T: Config> crate::verifier::SignedVerifier for Pallet<T> {
	type SolutionDataProvider = T::SolutionDataProvider;
	fn start() {
		StatusStorage::<T>::put(Status::Ongoing(crate::Pallet::<T>::msp()));
	}
}

impl<T: Config> Pallet<T> {
	fn do_on_initialize() -> Weight {
		if let Status::Ongoing(current_page) = Self::status_storage() {
			let page_solution =
				<T::SolutionDataProvider as SolutionDataProvider>::get_page(current_page);
			let maybe_supports =
				<Self as Verifier>::feasibility_check_page(page_solution, current_page);

			sublog!(
				debug,
				"verifier",
				"verified page {} of a solution, outcome = {:?}",
				current_page,
				maybe_supports.as_ref().map(|s| s.len())
			);

			match maybe_supports {
				Ok(supports) => {
					Self::deposit_event(Event::<T>::Verified(current_page, supports.len() as u32));
					QueuedSolution::<T>::set_invalid_page(current_page, supports);

					if current_page > crate::Pallet::<T>::lsp() {
						// not last page, just tick forward.
						StatusStorage::<T>::put(Status::Ongoing(current_page.saturating_sub(1)));
					} else {
						// last page, finalize everything.
						let claimed_score = T::SolutionDataProvider::get_score();
						match Self::finalize_verification(claimed_score) {
							Ok(_) => {
								T::SolutionDataProvider::report_result(VerificationResult::Valid);
							},
							Err(err) => {
								Self::deposit_event(Event::<T>::VerificationFailed(
									current_page,
									err,
								));
								QueuedSolution::<T>::clear_invalid();
								T::SolutionDataProvider::report_result(VerificationResult::Invalid)
							},
							// in both cases, we are not back to the nothing state.
						}
						StatusStorage::<T>::put(Status::Nothing);
					}
				},
				Err(err) => {
					// the page solution was invalid.
					Self::deposit_event(Event::<T>::VerificationFailed(current_page, err));
					StatusStorage::<T>::put(Status::Nothing);
					QueuedSolution::<T>::clear_invalid();
					T::SolutionDataProvider::report_result(VerificationResult::Invalid)
				},
			}
		}

		0
	}

	/// This should only be called when we are sure that no other page of `VerifyingSolutionStorage`
	/// needs verification.
	///
	/// Returns `Ok()` if everything is okay, at which point the valid variant of the queued
	/// solution will be updated, and the verifying solution will be removed. Returns
	/// `Err(Feasibility)` if any of the last verification steps fail.
	fn finalize_verification(claimed_score: ElectionScore) -> Result<(), FeasibilityError> {
		let outcome = QueuedSolution::<T>::final_score()
			.and_then(|(final_score, winner_count)| {
				let desired_targets = crate::Snapshot::<T>::desired_targets().unwrap();
				// claimed_score checked prior in seal_unverified_solution
				match (final_score == claimed_score, winner_count == desired_targets) {
					(true, true) => {
						// all good, finalize this solution
						// NOTE: must be before the call to `finalize_correct`.
						Self::deposit_event(Event::<T>::Queued(
							final_score,
							QueuedSolution::<T>::queued_solution(),
						));
						QueuedSolution::<T>::finalize_correct(final_score);
						Ok(())
					},
					(false, true) => Err(FeasibilityError::InvalidScore),
					(true, false) => Err(FeasibilityError::WrongWinnerCount),
					(false, false) => Err(FeasibilityError::InvalidScore),
				}
			})
			.map_err(|err| {
				sublog!(warn, "verifier", "Finalizing solution was invalid due to {:?}.", err);
				// In case of any of the errors, kill the solution.
				QueuedSolution::<T>::clear_invalid();
				err
			});
		sublog!(debug, "verifier", "finalize verification outcome: {:?}", outcome);
		outcome
	}

	// Ensure that the given score is:
	//
	// - better than the queued solution, if one exists.
	// - greater than the minimum untrusted score.
	pub(crate) fn ensure_score_quality(score: ElectionScore) -> Result<(), FeasibilityError> {
		let is_improvement = <Self as Verifier>::queued_solution().map_or(true, |best_score| {
			sp_npos_elections::is_score_better::<sp_runtime::Perbill>(
				score,
				best_score,
				T::SolutionImprovementThreshold::get(),
			)
		});
		ensure!(is_improvement, FeasibilityError::ScoreTooLow);

		let is_greater_than_min_trusted =
			Self::minimum_untrusted_score().map_or(true, |min_score| {
				sp_npos_elections::is_score_better(score, min_score, sp_runtime::Perbill::zero())
			});
		ensure!(is_greater_than_min_trusted, FeasibilityError::ScoreTooLow);

		Ok(())
	}

	pub(super) fn feasibility_check_page_inner(
		partial_solution: SolutionOf<T>,
		page: PageIndex,
	) -> Result<SupportsOf<Self>, FeasibilityError> {
		// Read the corresponding snapshots.
		let snapshot_targets =
			crate::Snapshot::<T>::targets().ok_or(FeasibilityError::SnapshotUnavailable)?;
		let snapshot_voters =
			crate::Snapshot::<T>::voters(page).ok_or(FeasibilityError::SnapshotUnavailable)?;

		// ----- Start building. First, we need some closures.
		let cache = helpers::generate_voter_cache::<T, _>(&snapshot_voters);
		let voter_at = helpers::voter_at_fn::<T>(&snapshot_voters);
		let target_at = helpers::target_at_fn::<T>(&snapshot_targets);
		let voter_index = helpers::voter_index_fn_usize::<T>(&cache);

		// Then convert solution -> assignment. This will fail if any of the indices are
		// gibberish.
		let assignments = partial_solution
			.into_assignment(voter_at, target_at)
			.map_err::<FeasibilityError, _>(Into::into)?;

		// Ensure that assignments are all correct.
		let _ = assignments
			.iter()
			.map(|ref assignment| {
				// Check that assignment.who is actually a voter (defensive-only). NOTE: while
				// using the index map from `voter_index` is better than a blind linear search,
				// this *still* has room for optimization. Note that we had the index when we
				// did `solution -> assignment` and we lost it. Ideal is to keep the index
				// around.

				// Defensive-only: must exist in the snapshot.
				let snapshot_index =
					voter_index(&assignment.who).ok_or(FeasibilityError::InvalidVoter)?;
				// Defensive-only: index comes from the snapshot, must exist.
				let (_voter, _stake, targets) =
					snapshot_voters.get(snapshot_index).ok_or(FeasibilityError::InvalidVoter)?;
				debug_assert!(*_voter == assignment.who);

				// Check that all of the targets are valid based on the snapshot.
				if assignment.distribution.iter().any(|(t, _)| !targets.contains(t)) {
					return Err(FeasibilityError::InvalidVote)
				}
				Ok(())
			})
			.collect::<Result<(), FeasibilityError>>()?;

		// ----- Start building support. First, we need one more closure.
		let stake_of = helpers::stake_of_fn::<T, _>(&snapshot_voters, &cache);

		// This might fail if the normalization fails. Very unlikely. See `integrity_test`.
		let staked_assignments =
			sp_npos_elections::assignment_ratio_to_staked_normalized(assignments, stake_of)
				.map_err::<FeasibilityError, _>(Into::into)?;

		let supports = sp_npos_elections::to_supports(&staked_assignments);

		// Ensure some heuristics. These conditions must hold in the **entire** support, this is
		// just a single page. But, they must hold in a single page as well.
		let desired_targets =
			crate::Snapshot::<T>::desired_targets().ok_or(FeasibilityError::SnapshotUnavailable)?;
		ensure!((supports.len() as u32) <= desired_targets, FeasibilityError::WrongWinnerCount);
		ensure!(
			supports
				.iter()
				.all(|(_, s)| (s.voters.len() as u32) <= T::MaxBackersPerWinner::get()),
			FeasibilityError::TooManyBackings
		);

		use frame_election_provider_support::TryIntoBoundedSupports;
		let bounded_supports = supports.try_into_bounded_supports().unwrap();
		Ok(bounded_supports)
	}

	#[cfg(test)]
	pub(crate) fn sanity_check() -> Result<(), &'static str> {
		Ok(())
	}
}

#[cfg(test)]
mod feasibility_check {
	use crate::{
		mock::*,
		types::*,
		verifier::{impls::Status, *},
		*,
	};
	// disambiguate event
	use crate::verifier::Event;

	use frame_election_provider_support::Support;
	use frame_support::{assert_noop, assert_ok};
	use sp_runtime::traits::Bounded;

	#[test]
	fn missing_snapshot() {
		ExtBuilder::default().build_unchecked().execute_with(|| {
			// create snapshot just so that we can create a solution..
			roll_to_snapshot_created();
			let paged = mine_full_solution().unwrap();

			// ..remove the only page of the target snapshot.
			crate::Snapshot::<Runtime>::remove_voter_page(0);

			assert_noop!(
				VerifierPallet::feasibility_check_page(paged.solution_pages[0].clone(), 0),
				FeasibilityError::SnapshotUnavailable
			);
		});

		ExtBuilder::default().pages(2).build_unchecked().execute_with(|| {
			// create snapshot just so that we can create a solution..
			roll_to_snapshot_created();
			let paged = mine_full_solution().unwrap();

			// ..remove just one of the pages of voter snapshot that is relevant.
			crate::Snapshot::<Runtime>::remove_voter_page(0);

			assert_noop!(
				VerifierPallet::feasibility_check_page(paged.solution_pages[0].clone(), 0),
				FeasibilityError::SnapshotUnavailable
			);
		});

		ExtBuilder::default().pages(2).build_unchecked().execute_with(|| {
			// create snapshot just so that we can create a solution..
			roll_to_snapshot_created();
			let paged = mine_full_solution().unwrap();

			// ..removing this page is not important.
			crate::Snapshot::<Runtime>::remove_voter_page(1);

			assert_ok!(VerifierPallet::feasibility_check_page(paged.solution_pages[0].clone(), 0));
		});

		ExtBuilder::default().pages(2).build_unchecked().execute_with(|| {
			// create snapshot just so that we can create a solution..
			roll_to_snapshot_created();
			let paged = mine_full_solution().unwrap();

			// `DesiredTargets` missing is also an error
			crate::Snapshot::<Runtime>::kill_desired_targets();

			assert_noop!(
				VerifierPallet::feasibility_check_page(paged.solution_pages[0].clone(), 0),
				FeasibilityError::SnapshotUnavailable
			);
		});

		ExtBuilder::default().pages(2).build_unchecked().execute_with(|| {
			// create snapshot just so that we can create a solution..
			roll_to_snapshot_created();
			roll_to(25);
			let paged = mine_full_solution().unwrap();

			// `DesiredTargets` is not checked here.
			crate::Snapshot::<Runtime>::remove_target_page(0);

			assert_noop!(
				VerifierPallet::feasibility_check_page(paged.solution_pages[1].clone(), 0),
				FeasibilityError::SnapshotUnavailable
			);
		});
	}

	#[test]
	fn winner_indices_single_page_must_be_in_bounds() {
		ExtBuilder::default().pages(1).desired_targets(2).build_and_execute(|| {
			roll_to_snapshot_created();
			let mut paged = mine_full_solution().unwrap();
			assert_eq!(crate::Snapshot::<Runtime>::targets().unwrap().len(), 4);
			// ----------------------------------------------------^^ valid range is [0..3].

			// Swap all votes from 3 to 4. here are only 4 targets, so index 4 is invalid.
			paged.solution_pages[0]
				.votes1
				.iter_mut()
				.filter(|(_, t)| *t == TargetIndex::from(3u16))
				.for_each(|(_, t)| *t += 1);

			assert_noop!(
				VerifierPallet::feasibility_check_page(paged.solution_pages[0].clone(), 0),
				FeasibilityError::NposElection(sp_npos_elections::Error::SolutionInvalidIndex)
			);
		})
	}

	#[test]
	fn voter_indices_per_page_must_be_in_bounds() {
		ExtBuilder::default()
			.pages(1)
			.voter_per_page(Bounded::max_value())
			.desired_targets(2)
			.build_and_execute(|| {
				roll_to_snapshot_created();
				let mut paged = mine_full_solution().unwrap();

				assert_eq!(crate::Snapshot::<Runtime>::voters(0).unwrap().len(), 12);
				// ------------------------------------------------^^ valid range is [0..11] in page
				// 0.

				// Check that there is an index 11 in votes1, and flip to 12. There are only 12
				// voters, so index 12 is invalid.
				assert!(
					paged.solution_pages[0]
						.votes1
						.iter_mut()
						.filter(|(v, _)| *v == VoterIndex::from(11u32))
						.map(|(v, _)| *v = 12)
						.count() > 0
				);
				assert_noop!(
					VerifierPallet::feasibility_check_page(paged.solution_pages[0].clone(), 0),
					FeasibilityError::NposElection(sp_npos_elections::Error::SolutionInvalidIndex),
				);
			})
	}

	#[test]
	fn voter_must_have_same_targets_as_snapshot() {
		ExtBuilder::default()
			.pages(1)
			.voter_per_page(Bounded::max_value())
			.desired_targets(2)
			.build_and_execute(|| {
				roll_to_snapshot_created();
				let mut paged = mine_full_solution().unwrap();

				// First, check that voter at index 11 (40) actually voted for 3 (40) -- this is
				// self vote. Then, change the vote to 2 (30).

				assert_eq!(
					paged.solution_pages[0]
						.votes1
						.iter_mut()
						.filter(|(v, t)| *v == 11 && *t == 3)
						.map(|(_, t)| *t = 2)
						.count(),
					1,
				);
				assert_noop!(
					VerifierPallet::feasibility_check_page(paged.solution_pages[0].clone(), 0),
					FeasibilityError::InvalidVote,
				);
			})
	}

	#[test]
	fn desired_targets() {
		ExtBuilder::default().max_backing_per_target(2).build_and_execute(|| {
			roll_to(25);
			ensure_full_snapshot();

			// valid solution that has only one winner. We can only detect this in the last page.
			let paged = raw_paged_from_supports(
				vec![vec![(40, Support { total: 20, voters: vec![(2, 10), (3, 10)] })]],
				0,
			);

			// initial state
			assert_eq!(VerifierPallet::status_storage(), Status::Nothing);
			assert!(MockSignedResults::get().is_empty());

			load_and_start_verification(paged);
			assert_eq!(VerifierPallet::status_storage(), Status::Ongoing(2));

			// now let it verify
			roll_to(26);
			assert_eq!(VerifierPallet::status_storage(), Status::Ongoing(1));
			roll_to(27);
			assert_eq!(VerifierPallet::status_storage(), Status::Ongoing(0));
			assert!(MockSignedResults::get().is_empty());
			roll_to(28);
			assert_eq!(VerifierPallet::status_storage(), Status::Nothing);

			// ..nothing is queued
			assert!(QueuedSolution::<Runtime>::queued_solution().is_none());
			// ..and these are our events.
			assert_eq!(
				verifier_events(),
				vec![
					Event::<Runtime>::Verified(2, 1), // msp has one winner, but no more.
					Event::<Runtime>::Verified(1, 0),
					Event::<Runtime>::Verified(0, 0),
					Event::<Runtime>::VerificationFailed(0, FeasibilityError::WrongWinnerCount),
				]
			);

			assert_eq!(MockSignedResults::get(), vec![VerificationResult::Invalid]);
		})
	}

	#[test]
	fn score() {
		ExtBuilder::default().build_and_execute(|| {
			roll_to_snapshot_created();
			let mut paged = mine_full_solution().unwrap();

			// just tweak score.
			paged.score[0] += 1;
			assert!(<VerifierPallet as Verifier>::queued_solution().is_none());

			load_and_start_verification(paged);
			roll_to_full_verification();

			// nothing is verified.
			assert!(<VerifierPallet as Verifier>::queued_solution().is_none());
			assert_eq!(
				verifier_events(),
				vec![
					Event::<Runtime>::Verified(2, 2),
					Event::<Runtime>::Verified(1, 2),
					Event::<Runtime>::Verified(0, 2),
					Event::<Runtime>::VerificationFailed(0, FeasibilityError::InvalidScore)
				]
			);

			assert_eq!(MockSignedResults::get(), vec![VerificationResult::Invalid]);
		})
	}

	#[test]
	fn heuristic_max_backers_per_winner_per_page() {
		ExtBuilder::default().max_backing_per_target(2).build_and_execute(|| {
			roll_to(25);
			ensure_full_snapshot();

			// these votes are all valid, but some dude has 3 supports in a single page.
			let solution = solution_from_supports(
				vec![(40, Support { total: 30, voters: vec![(2, 10), (3, 10), (4, 10)] })],
				// all these voters are in page of the snapshot, the msp!
				2,
			);
			let paged = PagedRawSolution {
				solution_pages: vec![solution].try_into().unwrap(),
				..Default::default()
			};
			load_and_start_verification(paged);
			assert_eq!(VerifierPallet::status(), Status::Ongoing(2));

			// now let it verify. It should fail on the first page.
			roll_to(26);

			// killing all storage items should be a noop, which is equal to saying: ensure none of
			// them exist anymore.
			assert_eq!(VerifierPallet::status(), Status::Nothing);
			assert_eq!(
				verifier_events(),
				vec![Event::<Runtime>::VerificationFailed(2, FeasibilityError::TooManyBackings)]
			);
			assert_eq!(MockSignedResults::get(), vec![VerificationResult::Invalid]);
		})
	}

	#[test]
	fn heuristic_desired_target_check_per_page() {
		ExtBuilder::default().desired_targets(2).build_and_execute(|| {
			roll_to(25);
			ensure_full_snapshot();

			// all of these votes are valid, but this solution is already presenting 3 winners,
			// while we just one 2.
			let solution = solution_from_supports(
				vec![
					(10, Support { total: 30, voters: vec![(4, 2)] }),
					(20, Support { total: 30, voters: vec![(4, 2)] }),
					(40, Support { total: 30, voters: vec![(4, 6)] }),
				],
				// all these voters are in page 2 of the snapshot, the msp!
				2,
			);
			let paged = PagedRawSolution {
				solution_pages: vec![solution].try_into().unwrap(),
				..Default::default()
			};
			load_and_start_verification(paged);
			assert_eq!(VerifierPallet::status(), Status::Ongoing(2));

			// now let it verify. It should fail on the first page.
			roll_to(26);

			assert_eq!(VerifierPallet::status(), Status::Nothing);
			assert_eq!(
				verifier_events(),
				vec![Event::<Runtime>::VerificationFailed(2, FeasibilityError::WrongWinnerCount)]
			);
			assert_eq!(MockSignedResults::get(), vec![VerificationResult::Invalid]);
		})
	}
}

#[cfg(test)]
mod misc {
	use super::*;
	use crate::mock::*;
	// disambiguate event
	use crate::verifier::Event;

	#[test]
	fn basic_single_verification_works() {
		ExtBuilder::default().pages(1).build_and_execute(|| {
			// load a solution after the snapshot has been created.
			roll_to(25);
			ensure_full_snapshot();

			let solution = mine_full_solution().unwrap();
			load_and_start_verification(solution);

			// now let it verify
			roll_to(26);

			// It done after just one block.
			assert_eq!(VerifierPallet::status(), Status::Nothing);
			assert_eq!(
				verifier_events(),
				vec![
					Event::<Runtime>::Verified(0, 2),
					Event::<Runtime>::Queued([15, 40, 850], None)
				]
			);
			assert_eq!(MockSignedResults::get(), vec![VerificationResult::Valid]);
		});
	}

	#[test]
	fn basic_multi_verification_works() {
		ExtBuilder::default().pages(3).build_and_execute(|| {
			// load a solution after the snapshot has been created.
			roll_to(25);
			ensure_full_snapshot();

			let solution = mine_full_solution().unwrap();
			// ------------- ^^^^^^^^^^^^

			load_and_start_verification(solution);
			assert_eq!(VerifierPallet::status(), Status::Ongoing(2));
			assert_eq!(QueuedSolution::<Runtime>::valid_iter().count(), 0);

			// now let it verify
			roll_to(26);
			assert_eq!(VerifierPallet::status(), Status::Ongoing(1));
			assert_eq!(verifier_events(), vec![Event::<Runtime>::Verified(2, 2)]);
			// 1 page verified, stored as invalid.
			assert_eq!(QueuedSolution::<Runtime>::invalid_iter().count(), 1);

			roll_to(27);
			assert_eq!(VerifierPallet::status(), Status::Ongoing(0));
			assert_eq!(
				verifier_events(),
				vec![Event::<Runtime>::Verified(2, 2), Event::<Runtime>::Verified(1, 2),]
			);
			// 2 pages verified, stored as invalid.
			assert_eq!(QueuedSolution::<Runtime>::invalid_iter().count(), 2);

			// nothing is queued yet.
			assert_eq!(MockSignedResults::get(), vec![]);
			assert_eq!(QueuedSolution::<Runtime>::valid_iter().count(), 0);
			assert!(QueuedSolution::<Runtime>::queued_solution().is_none());

			// last block.
			roll_to(28);
			assert_eq!(VerifierPallet::status(), Status::Nothing);
			assert_eq!(
				verifier_events(),
				vec![
					Event::<Runtime>::Verified(2, 2),
					Event::<Runtime>::Verified(1, 2),
					Event::<Runtime>::Verified(0, 2),
					Event::<Runtime>::Queued([55, 130, 8650], None),
				]
			);
			assert_eq!(MockSignedResults::get(), vec![VerificationResult::Valid]);

			// a solution has been queued
			assert_eq!(QueuedSolution::<Runtime>::valid_iter().count(), 3);
			assert!(QueuedSolution::<Runtime>::queued_solution().is_some());
		});
	}

	#[test]
	fn basic_multi_verification_partial() {
		ExtBuilder::default().pages(3).build_and_execute(|| {
			// load a solution after the snapshot has been created.
			roll_to(25);
			ensure_full_snapshot();

			let solution = mine_solution(2).unwrap();
			// -------------------------^^^

			load_and_start_verification(solution);

			assert_eq!(VerifierPallet::status(), Status::Ongoing(2));
			assert_eq!(QueuedSolution::<Runtime>::valid_iter().count(), 0);

			// now let it verify
			roll_to(26);
			assert_eq!(VerifierPallet::status(), Status::Ongoing(1));
			assert_eq!(verifier_events(), vec![Event::<Runtime>::Verified(2, 2)]);
			// 1 page verified, stored as invalid.
			assert_eq!(QueuedSolution::<Runtime>::invalid_iter().count(), 1);

			roll_to(27);
			assert_eq!(VerifierPallet::status(), Status::Ongoing(0));
			assert_eq!(
				verifier_events(),
				vec![Event::<Runtime>::Verified(2, 2), Event::<Runtime>::Verified(1, 2),]
			);
			// 2 page verified, stored as invalid.
			assert_eq!(QueuedSolution::<Runtime>::invalid_iter().count(), 2);

			// nothing is queued yet.
			assert_eq!(MockSignedResults::get(), vec![]);
			assert_eq!(QueuedSolution::<Runtime>::valid_iter().count(), 0);
			assert!(QueuedSolution::<Runtime>::queued_solution().is_none());

			roll_to(28);
			assert_eq!(VerifierPallet::status(), Status::Nothing);

			assert_eq!(
				verifier_events(),
				vec![
					Event::<Runtime>::Verified(2, 2),
					Event::<Runtime>::Verified(1, 2),
					// this is a partial solution, no one in this page (lsp).
					Event::<Runtime>::Verified(0, 0),
					Event::<Runtime>::Queued([30, 70, 2500], None),
				]
			);

			// a solution has been queued
			assert_eq!(MockSignedResults::get(), vec![VerificationResult::Valid]);
			assert_eq!(QueuedSolution::<Runtime>::valid_iter().count(), 3);
			assert!(QueuedSolution::<Runtime>::queued_solution().is_some());

			// page 0 is empty..
			assert_eq!(QueuedSolution::<Runtime>::get_valid_page(0).unwrap().len(), 0);
			// .. the other two are not.
			assert_eq!(QueuedSolution::<Runtime>::get_valid_page(1).unwrap().len(), 2);
			assert_eq!(QueuedSolution::<Runtime>::get_valid_page(2).unwrap().len(), 2);
		});
	}
}

// TODO: maybe a task for zeke.
// #[cfg(test)]
// mod verifier_trait {
// 	use super::*;
// 	use crate::{mock::*, types::Pagify, verifier::Verifier};

// 	#[test]
// 	fn setting_unverified_and_sealing_it() {
// 		ExtBuilder::default().pages(3).build_and_execute(|| {
// 			roll_to(25);
// 			let paged = mine_full_solution().unwrap();
// 			let score = paged.score.clone();

// 			for (page_index, solution_page) in paged.solution_pages.into_iter().enumerate() {
// 				assert_ok!(VerifierPallet::set_unverified_solution_page(
// 					page_index as PageIndex,
// 					solution_page,
// 				));
// 			}

// 			// after this, the pages should be set
// 			assert_ok!(VerifierPallet::seal_unverified_solution(paged.score.clone(),));

// 			assert_eq!(VerifyingSolution::<Runtime>::current_page(), Some(2));
// 			assert_eq!(VerifyingSolution::<Runtime>::get_score(), Some(score));
// 			assert_eq!(QueuedSolution::<Runtime>::invalid_iter().count(), 0);
// 			assert_eq!(QueuedSolution::<Runtime>::valid_iter().count(), 0);
// 			assert_eq!(QueuedSolution::<Runtime>::backing_iter().count(), 0);

// 			roll_to(26);

// 			// check the queued solution variants
// 			assert!(QueuedSolution::<Runtime>::get_invalid_page(2).is_some());
// 			assert_eq!(QueuedSolution::<Runtime>::invalid_iter().count(), 1);
// 			assert_eq!(QueuedSolution::<Runtime>::valid_iter().count(), 0);

// 			// check the backings
// 			assert!(QueuedSolution::<Runtime>::get_backing_page(2).is_some());
// 			assert_eq!(QueuedSolution::<Runtime>::backing_iter().count(), 1);

// 			// check the page cursor.
// 			assert_eq!(VerifyingSolution::<Runtime>::current_page(), Some(1));

// 			roll_to(27);

// 			// check the queued solution variants
// 			assert!(QueuedSolution::<Runtime>::get_invalid_page(1).is_some());
// 			assert!(QueuedSolution::<Runtime>::get_backing_page(1).is_some());
// 			assert_eq!(QueuedSolution::<Runtime>::invalid_iter().count(), 2);
// 			assert_eq!(QueuedSolution::<Runtime>::valid_iter().count(), 0);

// 			// check the backings
// 			assert!(QueuedSolution::<Runtime>::get_backing_page(1).is_some());
// 			assert_eq!(QueuedSolution::<Runtime>::backing_iter().count(), 2);

// 			// check the page cursor.
// 			assert_eq!(VerifyingSolution::<Runtime>::current_page(), Some(0));

// 			// now we finalize everything.
// 			roll_to(28);

// 			assert_eq!(QueuedSolution::<Runtime>::valid_iter().count(), 3);

// 			// invalid related queued solution data is cleared
// 			assert_eq!(QueuedSolution::<Runtime>::invalid_iter().count(), 0);
// 			assert_eq!(QueuedSolution::<Runtime>::backing_iter().count(), 0);

// 			// everything about the verifying solution is now removed.
// 			assert_eq!(VerifyingSolution::<Runtime>::current_page(), None);
// 			assert_eq!(VerifyingSolution::<Runtime>::get_score(), None);
// 			assert_eq!(VerifyingSolution::<Runtime>::iter().count(), 0);
// 		});
// 	}

// 	#[test]
// 	fn correct_solution_becomes_queued() {
// 		ExtBuilder::default().build_and_execute(|| {
// 			roll_to(25);
// 			let paged = mine_full_solution().unwrap();

// 			// set each page of the solution
// 			for (page_index, solution_page) in paged.solution_pages.into_iter().enumerate() {
// 				assert_ok!(VerifierPallet::set_unverified_solution_page(
// 					page_index as PageIndex,
// 					solution_page,
// 				));
// 			}

// 			// seal the solution
// 			assert_ok!(VerifierPallet::seal_unverified_solution(paged.score.clone(),));

// 			// load the last page of the solution
// 			roll_to(27);

// 			// the invalid queued solution is full
// 			assert_eq!(QueuedSolution::<Runtime>::invalid_iter().count(), 2);
// 			assert_eq!(QueuedSolution::<Runtime>::valid_iter().count(), 0);
// 			// and there is no queued solution
// 			assert_eq!(QueuedSolution::<Runtime>::queued_solution(), None);
// 			assert_eq!(QueuedSolution::<Runtime>::backing_iter().count(), 2);

// 			// now we finalize everything
// 			roll_to(28);

// 			// the solution becomes the valid solution
// 			assert_eq!(QueuedSolution::<Runtime>::valid_iter().count(), 3);
// 			assert_eq!(QueuedSolution::<Runtime>::invalid_iter().count(), 0);
// 			// which is also the queued solution
// 			assert!(matches!(QueuedSolution::<Runtime>::queued_solution(), Some(_)));

// 			// backing is cleared
// 			assert_eq!(QueuedSolution::<Runtime>::backing_iter().count(), 0);

// 			// everything about the verifying solution is now removed.
// 			assert_eq!(VerifyingSolution::<Runtime>::current_page(), None);
// 			assert_eq!(VerifyingSolution::<Runtime>::get_score(), None);
// 			assert_eq!(VerifyingSolution::<Runtime>::iter().count(), 0);
// 		});
// 	}

// 	#[test]
// 	fn incorrect_solution_is_discarded() {
// 		// first solution and invalid, should do nothing and make sure storage is totally cleared.
// 		ExtBuilder::default().pages(3).build_and_execute(|| {
// 			roll_to(25);
// 			let mut paged = mine_full_solution().unwrap();
// 			let score = paged.score.clone();

// 			// change a vote in the 2nd page to out an out-of-bounds target index
// 			assert_eq!(
// 				paged.solution_pages[1]
// 					.votes2
// 					.iter_mut()
// 					.filter(|(v, _, _)| *v == 0)
// 					.map(|(_, t, _)| t[0].0 = 4)
// 					.count(),
// 				1,
// 			);

// 			// set each page of the solution
// 			for (page_index, solution_page) in paged.solution_pages.into_iter().enumerate() {
// 				let page_index = page_index as PageIndex;
// 				assert_ok!(
// 					VerifierPallet::set_unverified_solution_page(page_index, solution_page,)
// 				);
// 			}

// 			// seal the solution
// 			assert_ok!(VerifierPallet::seal_unverified_solution(paged.score.clone(),));
// 			// thus full loading the verify solution
// 			assert_eq!(VerifyingSolution::<Runtime>::iter().count(), 3);
// 			assert_eq!(VerifyingSolution::<Runtime>::get_score(), Some(score));
// 			assert_eq!(VerifyingSolution::<Runtime>::current_page(), Some(2));

// 			// the queued solution is untouched
// 			assert_eq!(QueuedSolution::<Runtime>::invalid_iter().count(), 0);
// 			assert_eq!(QueuedSolution::<Runtime>::backing_iter().count(), 0);

// 			// verifying the 1st page is fine since it is valid
// 			roll_to(26);

// 			// cursor decrements by 1
// 			assert_eq!(VerifyingSolution::<Runtime>::current_page(), Some(1));
// 			// the queued solution has its first page
// 			assert_eq!(QueuedSolution::<Runtime>::invalid_iter().count(), 1);
// 			// and the target backings now include the first page
// 			assert!(QueuedSolution::<Runtime>::get_backing_page(2).is_some());
// 			assert_eq!(QueuedSolution::<Runtime>::backing_iter().count(), 1);

// 			// the 2nd page is rejected since it is invalid
// 			roll_to(27);

// 			// .. so the verifying solution is totally cleared
// 			assert_eq!(VerifyingSolution::<Runtime>::iter().count(), 0);
// 			assert_eq!(VerifyingSolution::<Runtime>::get_score(), None);
// 			assert_eq!(VerifyingSolution::<Runtime>::current_page(), None);

// 			// and the invalid backing solution is totally cleared/
// 			assert_eq!(QueuedSolution::<Runtime>::invalid_iter().count(), 0);
// 			assert_eq!(QueuedSolution::<Runtime>::backing_iter().count(), 0);
// 		});
// 	}

// 	#[test]
// 	fn better_solution_replaces_ok_solution() {
// 		// we have an ok solution, new better one comes along, we stored it.

// 		ExtBuilder::default().pages(3).build_and_execute(|| {
// 			roll_to(25);
// 			let good_paged = mine_full_solution().unwrap();
// 			let good_score = good_paged.score.clone();
// 			let ok_paged = raw_paged_solution_low_score();
// 			let ok_score = ok_paged.score.clone();

// 			// ensure the good solution is actually better than the ok solution
// 			assert!(good_score > ok_score);

// 			// set
// 			for (page_index, solution_page) in ok_paged.solution_pages.pagify(Pages::get()) {
// 				assert_ok!(VerifierPallet::set_unverified_solution_page(
// 					page_index,
// 					solution_page.clone(),
// 				));
// 			}
// 			// and seal the ok solution against the verifier
// 			assert_ok!(VerifierPallet::seal_unverified_solution(ok_score));

// 			// load the 2nd page of the ok solution
// 			roll_to(27);
// 			assert_eq!(QueuedSolution::<Runtime>::invalid_iter().count(), 2);
// 			assert_eq!(QueuedSolution::<Runtime>::valid_iter().count(), 0);

// 			// load the last page of the ok solution, and finalize it
// 			roll_to(28);

// 			// the valid solution and invalid are flipped
// 			assert_eq!(QueuedSolution::<Runtime>::invalid_iter().count(), 0);
// 			assert_eq!(QueuedSolution::<Runtime>::valid_iter().count(), 3);
// 			assert_eq!(<Runtime as crate::Config>::Verifier::queued_solution(), Some(ok_score));

// 			// everything about the verifying solution is now removed
// 			assert_eq!(VerifyingSolution::<Runtime>::current_page(), None);
// 			assert_eq!(VerifyingSolution::<Runtime>::get_score(), None);
// 			assert_eq!(VerifyingSolution::<Runtime>::iter().count(), 0);

// 			// the queued solutions backings are cleared
// 			assert_eq!(QueuedSolution::<Runtime>::backing_iter().count(), 0);

// 			for (page_index, solution_page) in good_paged.solution_pages.iter().enumerate() {
// 				assert_ok!(VerifierPallet::set_unverified_solution_page(
// 					page_index as PageIndex,
// 					solution_page.clone(),
// 				));
// 			}
// 			// and seal the good solution against the verifier
// 			assert_ok!(VerifierPallet::seal_unverified_solution(good_score,));

// 			// load the 2nd page of the good solution
// 			roll_to(30);

// 			// the invalid solution is the good solution
// 			assert_eq!(QueuedSolution::<Runtime>::invalid_iter().count(), 2);
// 			assert_eq!(VerifyingSolution::<Runtime>::get_score(), Some(good_score));

// 			// and the valid solution is still the ok one
// 			assert_eq!(QueuedSolution::<Runtime>::valid_iter().count(), 3);
// 			assert_eq!(<Runtime as crate::Config>::Verifier::queued_solution(), Some(ok_score));

// 			// finalize the good solution
// 			roll_to(31);

// 			// the invalid solution is cleared
// 			assert_eq!(QueuedSolution::<Runtime>::invalid_iter().count(), 0);

// 			// the good solution becomes the valid solution
// 			assert_eq!(QueuedSolution::<Runtime>::valid_iter().count(), 3);
// 			assert_eq!(<Runtime as crate::Config>::Verifier::queued_solution(), Some(good_score));

// 			// the verifying solution is now removed.
// 			assert_eq!(VerifyingSolution::<Runtime>::current_page(), None);
// 			assert_eq!(VerifyingSolution::<Runtime>::get_score(), None);
// 			assert_eq!(VerifyingSolution::<Runtime>::iter().count(), 0);

// 			// the queued solutions backings are cleared
// 			assert_eq!(QueuedSolution::<Runtime>::backing_iter().count(), 0);
// 		});
// 	}

// 	#[test]
// 	fn ok_solution_does_not_replace_good_solution() {
// 		ExtBuilder::default().pages(3).build_and_execute(|| {
// 			roll_to(25);
// 			let good_paged = mine_full_solution().unwrap();
// 			let good_score = good_paged.score.clone();
// 			let ok_paged = raw_paged_solution_low_score();
// 			let ok_score = ok_paged.score.clone();

// 			// ensure the good solution is actually better than the ok solution
// 			assert!(good_score > ok_score);

// 			// set
// 			for (page_index, solution_page) in good_paged.solution_pages.pagify(Pages::get()) {
// 				assert_ok!(VerifierPallet::set_unverified_solution_page(
// 					page_index,
// 					solution_page.clone(),
// 				));
// 			}
// 			// and seal the ok solution against the verifier
// 			assert_ok!(VerifierPallet::seal_unverified_solution(good_paged.score.clone(),));

// 			// load the last page of the ok solution, and finalize it
// 			roll_to(28);

// 			// the valid solution and invalid are flipped
// 			assert_eq!(QueuedSolution::<Runtime>::valid_iter().count(), 3);
// 			assert_eq!(<Runtime as crate::Config>::Verifier::queued_solution(), Some(good_score));

// 			// set
// 			for (page_index, solution_page) in ok_paged.solution_pages.pagify(Pages::get()) {
// 				assert_ok!(VerifierPallet::set_unverified_solution_page(
// 					page_index,
// 					solution_page.clone(),
// 				));
// 			}
// 			// and the solution will not be successfully sealed because the score is too low
// 			assert!(VerifierPallet::seal_unverified_solution(ok_score,).is_err());

// 			// the invalid solution is cleared
// 			assert_eq!(QueuedSolution::<Runtime>::invalid_iter().count(), 0);

// 			// the good solution is still the valid solution
// 			assert_eq!(QueuedSolution::<Runtime>::valid_iter().count(), 3);
// 			assert_eq!(<Runtime as crate::Config>::Verifier::queued_solution(), Some(good_score));

// 			// everything about the verifying solution is now removed.
// 			assert_eq!(VerifyingSolution::<Runtime>::current_page(), None);
// 			assert_eq!(VerifyingSolution::<Runtime>::get_score(), None);
// 			assert_eq!(VerifyingSolution::<Runtime>::iter().count(), 0);

// 			// the queued solutions backings are cleared
// 			assert_eq!(QueuedSolution::<Runtime>::backing_iter().count(), 0);
// 		});
// 	}

// 	#[test]
// 	fn incorrect_solution_does_not_mess_with_queued() {
// 		// we have a good solution, bad one comes along, we discard it safely
// 		ExtBuilder::default().pages(3).build_and_execute(|| {
// 			roll_to(25);

// 			let paged = mine_full_solution().unwrap();
// 			let score = paged.score.clone();
// 			assert_eq!(score, [55, 130, 8650,]);

// 			let mut bad_paged = mine_full_solution().unwrap();
// 			bad_paged.score = [54, 129, 8640];

// 			// change a vote in the 2nd page to out an out-of-bounds target index
// 			assert_eq!(
// 				bad_paged.solution_pages[1]
// 					.votes2
// 					.iter_mut()
// 					.filter(|(v, _, _)| *v == 0)
// 					.map(|(_, t, _)| t[0].0 = 4)
// 					.count(),
// 				1,
// 			);

// 			// set
// 			for (page_index, solution_page) in paged.solution_pages.pagify(Pages::get()) {
// 				assert_ok!(VerifierPallet::set_unverified_solution_page(
// 					page_index,
// 					solution_page.clone(),
// 				));
// 			}
// 			// and seal the solution against the verifier
// 			assert_ok!(VerifierPallet::seal_unverified_solution(score));

// 			// finalize the solution
// 			roll_to(28);
// 			assert_eq!(QueuedSolution::<Runtime>::valid_iter().count(), 3);
// 			assert_eq!(<Runtime as crate::Config>::Verifier::queued_solution(), Some(score));

// 			// set
// 			for (page_index, solution_page) in bad_paged.solution_pages.pagify(Pages::get()) {
// 				assert_ok!(VerifierPallet::set_unverified_solution_page(
// 					page_index,
// 					solution_page.clone(),
// 				));
// 			}
// 			// and the bad solution cannot be successfully sealed against the verifier because it
// 			// has a bad score.
// 			assert!(VerifierPallet::seal_unverified_solution(bad_paged.score.clone(),).is_err());

// 			// then the verifying solution storage is wiped
// 			assert_eq!(<Runtime as crate::Config>::Verifier::queued_solution(), Some(score));

// 			// everything about the verifying solution is removed
// 			assert_eq!(VerifyingSolution::<Runtime>::current_page(), None);
// 			assert_eq!(VerifyingSolution::<Runtime>::get_score(), None);
// 			assert_eq!(VerifyingSolution::<Runtime>::iter().count(), 0);

// 			// the queued solutions backings are cleared
// 			assert_eq!(QueuedSolution::<Runtime>::backing_iter().count(), 0);

// 			// the valid solution is unchanged
// 			assert_eq!(QueuedSolution::<Runtime>::valid_iter().count(), 3);
// 		});
// 	}

// 	#[test]
// 	fn rejects_new_verification_if_ongoing() {
// 		todo!("if there's already some verification ongoing, then we don't accept new ones");
// 		// not sure what to do about `force_set_single_page_valid`
// 	}
// }
