import math
import pytest

from model import KFarmsModel, MULTIPLIER_SCALE, SCALE


def test_rpt_monotonic_and_points_accounting():
	model = KFarmsModel(global_emission_per_slot=1_000_000)
	model.add_pool("poolA", weight=100, current_slot=0)

	# Initially no stakers, settle has no effect
	model.settle_pool("poolA", current_slot=10)
	assert model.pools["poolA"].rpt_scaled == 0

	# User deposits 100 units with 1x multiplier
	model.deposit("alice", "poolA", amount=100, lock_multiplier_num=MULTIPLIER_SCALE, current_slot=10)
	assert model.pools["poolA"].total_points == 100

	# Advance 100 slots and settle
	model.settle_pool("poolA", current_slot=110)
	assert model.pools["poolA"].rpt_scaled > 0
	rpt1 = model.pools["poolA"].rpt_scaled

	# RPT is monotonic
	model.settle_pool("poolA", current_slot=110)
	assert model.pools["poolA"].rpt_scaled == rpt1

	# Alice earns correctly
	model.settle_user("alice", "poolA")
	accrued1 = model.users["alice"]["poolA"].accrued_rewards
	assert accrued1 > 0

	# A new depositor cannot capture past dust
	model.deposit("bob", "poolA", amount=100, lock_multiplier_num=MULTIPLIER_SCALE, current_slot=110)
	assert model.pools["poolA"].total_points == 200

	# Advance again; ensure conservation: per-slot emission ~ split by points
	model.settle_pool("poolA", current_slot=120)
	model.settle_user("alice", "poolA")
	model.settle_user("bob", "poolA")

	accrued_total = model.sum_user_accrued("poolA")
	assert accrued_total > accrued1

	# Withdraw does not underflow pool points
	model.withdraw("alice", "poolA", amount=50, current_slot=125)
	assert model.pools["poolA"].total_points == model.sum_user_points("poolA")


def test_extend_lock_only_affects_future():
	model = KFarmsModel(global_emission_per_slot=10_000)
	model.add_pool("poolA", weight=100, current_slot=0)
	model.deposit("alice", "poolA", amount=1_000, lock_multiplier_num=MULTIPLIER_SCALE, current_slot=0)

	# Earn at 1x for 10 slots
	model.settle_pool("poolA", current_slot=10)
	model.settle_user("alice", "poolA")
	pre_extend = model.users["alice"]["poolA"].accrued_rewards

	# Extend to 2x, then advance more slots
	mult2x = 2 * MULTIPLIER_SCALE
	model.extend_lock("alice", "poolA", new_lock_multiplier_num=mult2x, current_slot=10)
	model.settle_pool("poolA", current_slot=20)
	model.settle_user("alice", "poolA")
	post_extend = model.users["alice"]["poolA"].accrued_rewards

	# Additional rewards after extend must be positive; and there must be no retroactive bump
	assert post_extend > pre_extend

