import pytest

from model import (
	KFarmsModel,
	MULTIPLIER_SCALE,
	SCALE,
	ArithmeticErrorOutOfRange,
)


def test_dust_siphon_pattern():
	# Attacker tries to capture past dust by depositing right after a long settle window
	E = 10_000
	model = KFarmsModel(global_emission_per_slot=E)
	model.add_pool("pool", weight=100, current_slot=0)

	# Existing staker accrues for 100 slots
	model.deposit("victor", "pool", amount=1_000, lock_multiplier_num=MULTIPLIER_SCALE, current_slot=0)
	model.settle_pool("pool", current_slot=100)

	# Attacker deposits exactly at slot 100 and immediately claims
	model.deposit("attacker", "pool", amount=1, lock_multiplier_num=MULTIPLIER_SCALE, current_slot=100)
	claim_immediate = model.claim("attacker", "pool", current_slot=100)

	# If dust siphon existed, attacker would get non-zero from historical dust
	assert claim_immediate == 0

	# Now move one slot and ensure attacker only earns for that 1 slot proportionally
	model.settle_pool("pool", current_slot=101)
	claim_one_slot = model.claim("attacker", "pool", current_slot=101)
	assert claim_one_slot > 0
	# Upper bound by total emission per slot (attacker points <= total points)
	assert claim_one_slot <= E


def test_external_points_forgery():
	# Demonstrate that an unsafe external "set_points" that does not align paid_rpt is exploitable
	E = 5_000
	model = KFarmsModel(global_emission_per_slot=E)
	model.add_pool("pool", weight=100, current_slot=0)

	# Honest user accrues for 50 slots to build a positive RPT
	model.deposit("honest", "pool", amount=1_000, lock_multiplier_num=MULTIPLIER_SCALE, current_slot=0)
	model.settle_pool("pool", current_slot=50)

	# Simulate a malicious external setter writing attacker points without settle/payout alignment
	pool = model.pools["pool"]
	att_pos = model.ensure_user_pos("attacker", "pool")
	# Unsafe: increase points and pool total_points, but DO NOT set paid_rpt_scaled to current RPT
	att_pos.points += 1_000
	pool.total_points += 1_000

	# When we now settle attacker, they capture past RPT they never earned
	model.settle_user("attacker", "pool")
	ill_gotten = model.claim("attacker", "pool", current_slot=50)
	assert ill_gotten > 0, "Attacker claimed past rewards due to unsafe points write"

	# Safe pattern via deposit aligns paid_rpt, preventing retroactive accrual
	model.deposit("defender", "pool", amount=1_000, lock_multiplier_num=MULTIPLIER_SCALE, current_slot=50)
	claim_defender = model.claim("defender", "pool", current_slot=50)
	assert claim_defender == 0


def test_overflow_amplification():
	# Ensure u128 bounds prevent overflow-based points amplification
	model = KFarmsModel(global_emission_per_slot=1)
	model.add_pool("pool", weight=1, current_slot=0)

	# Choose stake so that stake*multiplier exceeds u128
	too_large_stake = ((1 << 128) // MULTIPLIER_SCALE) + 1
	with pytest.raises(ArithmeticErrorOutOfRange):
		model.deposit("alice", "pool", amount=int(too_large_stake), lock_multiplier_num=MULTIPLIER_SCALE, current_slot=0)


def test_p_i_zero_trap():
	# Emissions while P_i=0 must not be stockpiled and claimable by the next staker
	E = 7_000
	model = KFarmsModel(global_emission_per_slot=E)
	model.add_pool("pool", weight=100, current_slot=0)

	# Accrue some rewards with a staker, then withdraw to make P=0
	model.deposit("victor", "pool", amount=10_000, lock_multiplier_num=MULTIPLIER_SCALE, current_slot=0)
	model.settle_pool("pool", current_slot=10)
	model.withdraw("victor", "pool", amount=10_000, current_slot=10)

	# Advance a long time with P=0; pool should not accumulate payable rewards
	model.settle_pool("pool", current_slot=1010)

	# New tiny staker joins; should NOT capture the prior 1000 slots worth of emission
	model.deposit("attacker", "pool", amount=1, lock_multiplier_num=MULTIPLIER_SCALE, current_slot=1010)
	# One more slot passes
	model.settle_pool("pool", current_slot=1011)
	claim = model.claim("attacker", "pool", current_slot=1011)
	assert claim <= E, "Attacker improperly captured emissions from P=0 interval"


def test_time_warp_overpay():
	# Large Δslot should be clamped to MAX_SLOTS_PER_UPDATE inside the model
	from model.kfarms_model import MAX_SLOTS_PER_UPDATE

	E = 100_000
	model = KFarmsModel(global_emission_per_slot=E)
	model.add_pool("pool", weight=100, current_slot=0)
	model.deposit("alice", "pool", amount=1_000, lock_multiplier_num=MULTIPLIER_SCALE, current_slot=0)

	pool = model.pools["pool"]
	rpt_before = pool.rpt_scaled

	# Jump far ahead; internal clamp should bound the increase
	model.settle_pool("pool", current_slot=10_000_000)
	rpt_after = pool.rpt_scaled

	# Compute expected upper bound using clamp
	pool_emission_per_slot = (E * pool.weight) // model.total_weight
	points = pool.total_points
	expected_drpt = (pool_emission_per_slot * MAX_SLOTS_PER_UPDATE * SCALE) // points
	assert rpt_after - rpt_before <= expected_drpt


def test_insolvency_freeze():
	# Demonstrate insolvency risk if claims are not pro-rated when vault underfunded
	E = 1_000_000
	model = KFarmsModel(global_emission_per_slot=E)
	model.add_pool("pool", weight=100, current_slot=0)
	model.deposit("alice", "pool", amount=1_000, lock_multiplier_num=MULTIPLIER_SCALE, current_slot=0)
	model.deposit("bob", "pool", amount=1_000, lock_multiplier_num=MULTIPLIER_SCALE, current_slot=0)

	# Accrue for 1,000 slots
	model.settle_pool("pool", current_slot=1000)
	model.settle_user("alice", "pool")
	model.settle_user("bob", "pool")

	owed_alice = model.claim("alice", "pool", current_slot=1000)
	owed_bob = model.claim("bob", "pool", current_slot=1000)
	total_owed = owed_alice + owed_bob

	# Suppose the reward vault only has half the owed amount funded
	vault_funded = total_owed // 2

	# If claims are not pro-rated, first claim can drain vault and second would revert on-chain
	remaining_after_alice = max(0, vault_funded - owed_alice)
	assert owed_bob > remaining_after_alice, "Without pro-rata, second claimant would revert due to insolvency"

