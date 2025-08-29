from dataclasses import dataclass
from typing import Dict, Optional


SCALE: int = 10 ** 18
MULTIPLIER_SCALE: int = 10 ** 6
MAX_U128: int = (1 << 128) - 1
MAX_SLOTS_PER_UPDATE: int = 8192


class ArithmeticErrorOutOfRange(Exception):
	pass


def assert_u128(value: int, field_name: str = "value") -> None:
	if value < 0 or value > MAX_U128:
		raise ArithmeticErrorOutOfRange(f"{field_name} exceeds u128 bounds: {value}")


def floor_div(numerator: int, denominator: int) -> int:
	if denominator == 0:
		return 0
	return numerator // denominator


@dataclass
class Pool:
	weight: int
	last_update_slot: int = 0
	rpt_scaled: int = 0
	total_points: int = 0
	dust_reservoir: int = 0

	def settle_pool(self, global_emission_per_slot: int, total_weight: int, current_slot: int) -> None:
		if current_slot < self.last_update_slot:
			# Monotonic slot assumption; ignore backward time
			return
		delta_slots_raw: int = current_slot - self.last_update_slot
		delta_slots: int = min(delta_slots_raw, MAX_SLOTS_PER_UPDATE)
		if self.total_points > 0 and self.weight > 0 and total_weight > 0 and delta_slots > 0:
			pool_emission_per_slot: int = floor_div(global_emission_per_slot * self.weight, total_weight)
			assert_u128(pool_emission_per_slot, "pool_emission_per_slot")
			rewards: int = pool_emission_per_slot * delta_slots
			assert_u128(rewards, "rewards")
			# Add dust reservoir to ensure dust is not claimable by fresh stakers
			numerator: int = rewards * SCALE + self.dust_reservoir
			# Do not bound numerator with u128 here; SCALE can exceed, but results must fit u128 post-division
			drpt: int = floor_div(numerator, self.total_points)
			# Update dust as remainder to keep conservation
			self.dust_reservoir = numerator - drpt * self.total_points
			self.rpt_scaled = self.rpt_scaled + drpt
			assert_u128(self.rpt_scaled, "rpt_scaled")
		self.last_update_slot = current_slot


@dataclass
class UserPosition:
	staked_amount: int = 0
	lock_multiplier_num: int = MULTIPLIER_SCALE
	points: int = 0
	paid_rpt_scaled: int = 0
	accrued_rewards: int = 0

	def compute_points(self, new_stake: Optional[int] = None, new_multiplier_num: Optional[int] = None) -> int:
		stake: int = self.staked_amount if new_stake is None else new_stake
		mult: int = self.lock_multiplier_num if new_multiplier_num is None else new_multiplier_num
		# points = stake * mult / MULTIPLIER_SCALE
		product: int = stake * mult
		assert_u128(product, "stake*mult")
		pts: int = floor_div(product, MULTIPLIER_SCALE)
		assert_u128(pts, "points")
		return pts


class KFarmsModel:
	def __init__(self, global_emission_per_slot: int):
		self.global_emission_per_slot: int = global_emission_per_slot
		self.pools: Dict[str, Pool] = {}
		self.users: Dict[str, Dict[str, UserPosition]] = {}
		self.total_weight: int = 0

	def add_pool(self, pool_id: str, weight: int, current_slot: int) -> None:
		if pool_id in self.pools:
			raise ValueError("Pool already exists")
		self.pools[pool_id] = Pool(weight=weight, last_update_slot=current_slot)
		self.total_weight += weight

	def set_pool_weight(self, pool_id: str, new_weight: int, current_slot: int) -> None:
		pool = self.pools[pool_id]
		pool.settle_pool(self.global_emission_per_slot, self.total_weight, current_slot)
		self.total_weight = self.total_weight - pool.weight + new_weight
		pool.weight = new_weight

	def ensure_user(self, user: str) -> None:
		if user not in self.users:
			self.users[user] = {}

	def ensure_user_pos(self, user: str, pool_id: str) -> UserPosition:
		self.ensure_user(user)
		if pool_id not in self.users[user]:
			self.users[user][pool_id] = UserPosition()
		return self.users[user][pool_id]

	def settle_pool(self, pool_id: str, current_slot: int) -> None:
		pool = self.pools[pool_id]
		pool.settle_pool(self.global_emission_per_slot, self.total_weight, current_slot)

	def settle_user(self, user: str, pool_id: str) -> None:
		pool = self.pools[pool_id]
		pos = self.ensure_user_pos(user, pool_id)
		delta_rpt: int = pool.rpt_scaled - pos.paid_rpt_scaled
		if delta_rpt < 0:
			delta_rpt = 0
		increment: int = floor_div(pos.points * delta_rpt, SCALE)
		assert_u128(increment, "user_accrued_increment")
		pos.accrued_rewards += increment
		assert_u128(pos.accrued_rewards, "user_accrued_rewards")
		pos.paid_rpt_scaled = pool.rpt_scaled

	def deposit(self, user: str, pool_id: str, amount: int, lock_multiplier_num: Optional[int], current_slot: int) -> None:
		if amount < 0:
			raise ValueError("amount must be non-negative")
		pool = self.pools[pool_id]
		self.settle_pool(pool_id, current_slot)
		pos = self.ensure_user_pos(user, pool_id)
		self.settle_user(user, pool_id)
		# Update stake and multiplier (if provided) then recompute points
		new_mult = pos.lock_multiplier_num if lock_multiplier_num is None else lock_multiplier_num
		if new_mult < MULTIPLIER_SCALE:
			# Enforce multiplier >= 1x
			new_mult = MULTIPLIER_SCALE
		pos.staked_amount += amount
		assert_u128(pos.staked_amount, "staked_amount")
		pos.lock_multiplier_num = new_mult
		new_points: int = pos.compute_points()
		delta_points: int = new_points - pos.points
		if delta_points != 0:
			pool.total_points += delta_points
			assert_u128(pool.total_points, "pool_total_points")
			pos.points = new_points
		# After mutation, align paid RPT to current to avoid retroactive accrual on new points
		pos.paid_rpt_scaled = pool.rpt_scaled

	def extend_lock(self, user: str, pool_id: str, new_lock_multiplier_num: int, current_slot: int) -> None:
		pool = self.pools[pool_id]
		self.settle_pool(pool_id, current_slot)
		pos = self.ensure_user_pos(user, pool_id)
		self.settle_user(user, pool_id)
		if new_lock_multiplier_num < MULTIPLIER_SCALE:
			raise ValueError("lock multiplier cannot be below 1x")
		new_points: int = pos.compute_points(new_multiplier_num=new_lock_multiplier_num)
		delta_points: int = new_points - pos.points
		if delta_points != 0:
			pool.total_points += delta_points
			assert_u128(pool.total_points, "pool_total_points")
			pos.points = new_points
		pos.lock_multiplier_num = new_lock_multiplier_num
		pos.paid_rpt_scaled = pool.rpt_scaled

	def withdraw(self, user: str, pool_id: str, amount: int, current_slot: int) -> None:
		pool = self.pools[pool_id]
		self.settle_pool(pool_id, current_slot)
		pos = self.ensure_user_pos(user, pool_id)
		self.settle_user(user, pool_id)
		if amount > pos.staked_amount:
			raise ValueError("cannot withdraw more than staked")
		pos.staked_amount -= amount
		new_points: int = pos.compute_points()
		delta_points: int = new_points - pos.points
		if delta_points != 0:
			pool.total_points += delta_points
			if pool.total_points < 0:
				raise ArithmeticErrorOutOfRange("pool total points underflow")
			pos.points = new_points
		pos.paid_rpt_scaled = pool.rpt_scaled

	def claim(self, user: str, pool_id: str, current_slot: int) -> int:
		self.settle_pool(pool_id, current_slot)
		self.settle_user(user, pool_id)
		pos = self.ensure_user_pos(user, pool_id)
		amount: int = pos.accrued_rewards
		pos.accrued_rewards = 0
		return amount

	def sum_user_points(self, pool_id: str) -> int:
		return sum(pos.points for pos in (self.users.get(u, {}).get(pool_id) for u in self.users) if pos is not None)

	def sum_user_accrued(self, pool_id: Optional[str] = None) -> int:
		if pool_id is None:
			return sum(pos.accrued_rewards for u in self.users for pos in self.users[u].values())
		return sum(pos.accrued_rewards for u in self.users for pid, pos in self.users[u].items() if pid == pool_id)

