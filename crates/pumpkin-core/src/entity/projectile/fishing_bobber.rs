use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use crate::entity::projectile::{ProjectileHit, is_projectile};
use crate::world::World;
use crate::{
    entity::{Entity, EntityBase, living::LivingEntity, player::Player},
    server::Server,
};
use pumpkin_data::block_properties::WaterLikeProperties;
use pumpkin_data::fluid::Fluid;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::{Block, BlockState};
use pumpkin_util::math::boundingbox::BoundingBox;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;

pub struct FishingBobberEntity {
    pub entity: Entity,
    pub owner_id: i32,
    pub hooked_entity_id: AtomicI32,
    pub in_ground: AtomicBool,
    pub has_hit: AtomicBool,
    pub wait_countdown: AtomicI32,
    pub bite_countdown: AtomicI32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FishingCatchPlan {
    pub pull_hooked: bool,
    pub roll_loot: bool,
    pub experience: i32,
    pub rod_damage: i32,
}

#[must_use]
pub fn fishing_catch_plan(
    hooked: bool,
    bite_ready: bool,
    open_water: bool,
    experience_roll: i32,
) -> FishingCatchPlan {
    if hooked {
        return FishingCatchPlan {
            pull_hooked: true,
            roll_loot: false,
            experience: 0,
            rod_damage: 0,
        };
    }
    if bite_ready && open_water {
        return FishingCatchPlan {
            pull_hooked: false,
            roll_loot: true,
            experience: experience_roll.clamp(1, 6),
            rod_damage: 1,
        };
    }
    FishingCatchPlan {
        pull_hooked: false,
        roll_loot: false,
        experience: 0,
        rod_damage: 0,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FishingLayer {
    Water,
    Above,
    Blocked,
}

fn layers_are_open_water(layers: &[FishingLayer]) -> bool {
    let mut seen_water = false;
    let mut seen_above = false;
    for layer in layers {
        match *layer {
            FishingLayer::Blocked => return false,
            FishingLayer::Water => {
                if seen_above {
                    return false;
                }
                seen_water = true;
            }
            FishingLayer::Above => {
                if !seen_water {
                    return false;
                }
                seen_above = true;
            }
        }
    }
    seen_water && seen_above
}

fn random_wait() -> i32 {
    100 + i32::try_from(rand::random::<u32>() % 600).unwrap_or(0)
}

fn random_fishing_experience() -> i32 {
    1 + i32::try_from(rand::random::<u32>() % 6).unwrap_or(0)
}

fn block_is_water(world: &World, pos: BlockPos) -> bool {
    world.get_fluid(&pos).matches_type(&Fluid::WATER)
}

fn is_source_water(state: &BlockState) -> bool {
    Block::from_state_id(state.id) == &Block::WATER
        && WaterLikeProperties::from_state_id(state.id).level == 0
}

fn is_flowing_water_block(state: &BlockState) -> bool {
    Block::from_state_id(state.id) == &Block::WATER
        && WaterLikeProperties::from_state_id(state.id).level != 0
}

fn fishing_cell(world: &World, pos: BlockPos) -> FishingLayer {
    let state = world.get_block_state(&pos);
    let fluid = world.get_fluid(&pos);
    let blocks_movement = state.get_block_collision_shapes().next().is_some();
    if is_source_water(state)
        || (fluid.matches_type(&Fluid::WATER) && !blocks_movement && !is_flowing_water_block(state))
    {
        return FishingLayer::Water;
    }
    if blocks_movement {
        FishingLayer::Blocked
    } else {
        FishingLayer::Above
    }
}

fn layer_at(world: &World, origin: BlockPos, dy: i32) -> FishingLayer {
    let mut saw_water = false;
    let mut saw_above = false;
    for dx in -2i32..=2 {
        for dz in -2i32..=2 {
            let pos = BlockPos::new(origin.0.x + dx, origin.0.y + dy, origin.0.z + dz);
            match fishing_cell(world, pos) {
                FishingLayer::Blocked => return FishingLayer::Blocked,
                FishingLayer::Water => {
                    if saw_above {
                        return FishingLayer::Blocked;
                    }
                    saw_water = true;
                }
                FishingLayer::Above => {
                    if saw_water {
                        return FishingLayer::Blocked;
                    }
                    saw_above = true;
                }
            }
        }
    }
    if saw_water {
        FishingLayer::Water
    } else {
        FishingLayer::Above
    }
}

fn position_is_open_water(world: &World, pos: Vector3<f64>) -> bool {
    let origin = BlockPos::floored(pos.x, pos.y, pos.z);
    let layers = [
        layer_at(world, origin, -1),
        layer_at(world, origin, 0),
        layer_at(world, origin, 1),
        layer_at(world, origin, 2),
    ];
    layers_are_open_water(&layers)
}

fn segment_box(start: Vector3<f64>, end: Vector3<f64>) -> BoundingBox {
    BoundingBox::new(
        Vector3::new(start.x.min(end.x), start.y.min(end.y), start.z.min(end.z)),
        Vector3::new(start.x.max(end.x), start.y.max(end.y), start.z.max(end.z)),
    )
    .expand(0.3, 0.3, 0.3)
}

fn first_water_along(
    world: &World,
    start: Vector3<f64>,
    end: Vector3<f64>,
) -> Option<Vector3<f64>> {
    let delta = end - start;
    let length = delta.length();
    if length < 1.0e-4 {
        return None;
    }
    let mut traveled = 0.0;
    while traveled < length {
        let advance = (length - traveled).min(0.25);
        traveled += advance;
        let t = traveled / length;
        let point = start.add(&delta.multiply(t, t, t));
        if block_is_water(world, BlockPos::floored(point.x, point.y, point.z)) {
            return Some(point);
        }
    }
    None
}

fn snap_into_water(point: Vector3<f64>) -> Vector3<f64> {
    let mut landed = point;
    landed.y = landed.y.floor() + 0.6;
    landed
}

fn float_on_surface(world: &World, pos: Vector3<f64>) -> Vector3<f64> {
    let block = BlockPos::floored(pos.x, pos.y, pos.z);
    let above = BlockPos::new(block.0.x, block.0.y + 1, block.0.z);
    if block_is_water(world, above) {
        return pos;
    }
    let mut floated = pos;
    floated.y = f64::from(block.0.y) + 0.6;
    floated
}

impl FishingBobberEntity {
    const WATER_INERTIA: f64 = 0.8;
    const AIR_INERTIA: f64 = 0.92;
    const GRAVITY: f64 = 0.03;
    const CAST_SPEED: f64 = 1.5;
    const BITE_WINDOW: i32 = 40;

    pub fn new(entity: Entity, owner: &Player) -> Self {
        let owner_id = owner.entity_id();
        let mut owner_pos = owner.living_entity.entity.pos.load();
        owner_pos.y += owner.living_entity.entity.get_eye_height() - 0.1;
        entity.pos.store(owner_pos);
        entity.last_sent_pos.store(owner_pos);
        entity.data.store(owner_id, Ordering::Relaxed);

        Self {
            entity,
            owner_id,
            hooked_entity_id: AtomicI32::new(0),
            in_ground: AtomicBool::new(false),
            has_hit: AtomicBool::new(false),
            wait_countdown: AtomicI32::new(random_wait()),
            bite_countdown: AtomicI32::new(0),
        }
    }

    pub fn shoot_towards(&self, yaw: f32, pitch: f32) {
        let look = Vector3::from_yaw_pitch(yaw, pitch);
        let velocity = look.multiply(Self::CAST_SPEED, Self::CAST_SPEED, Self::CAST_SPEED);
        self.entity.velocity.store(velocity);
        self.entity.last_sent_velocity.store(velocity);
    }

    pub fn reel_in(&self, player: &Player) -> i32 {
        let world = self.entity.world.load();
        let hooked_id = self.hooked_entity_id.load(Ordering::Relaxed);
        let hooked_alive = hooked_id != 0 && world.get_entity_by_id(hooked_id).is_some();
        let pos = self.entity.pos.load();
        let in_water = block_is_water(&world, BlockPos::floored(pos.x, pos.y, pos.z));
        let open_water = in_water && position_is_open_water(&world, pos);
        let biting = self.bite_countdown.load(Ordering::Relaxed) > 0;
        let plan = fishing_catch_plan(
            hooked_alive,
            biting,
            open_water,
            random_fishing_experience(),
        );

        if plan.pull_hooked {
            Self::pull_hooked(player, &world, hooked_id);
            return 0;
        }

        if !plan.roll_loot {
            return 0;
        }

        let stacks = crate::world::loot::generate_loot(
            &pumpkin_data::loot_table::GAMEPLAY_FISHING,
            rand::random::<i64>(),
        );
        let Some(first) = stacks.first() else {
            return 0;
        };
        let caught_id = format!("minecraft:{}", first.item.registry_key);

        for mut stack in stacks {
            player.inventory().insert_stack_anywhere(&mut stack);
            if !stack.is_empty() {
                player.drop_item(stack);
            }
        }
        player.sync_inventory_to_client();

        player.increment_stat(
            pumpkin_data::statistic::StatisticCategory::Custom,
            pumpkin_data::statistic::CustomStatistic::FishCaught as i32,
            1,
        );
        player.trigger_advancement(
            crate::entity::player::advancement::trigger::AdvancementTrigger::FishedItem {
                item_id: caught_id,
            },
        );
        Self::grant_experience(player, &world, plan.experience);
        world.play_sound(
            Sound::EntityExperienceOrbPickup,
            SoundCategory::Neutral,
            &player.position(),
        );
        plan.rod_damage
    }

    fn pull_hooked(player: &Player, world: &World, hooked_id: i32) {
        let Some(hooked) = world.get_entity_by_id(hooked_id) else {
            return;
        };
        if let Some(item) = hooked.get_item_entity() {
            item.set_pickup_delay(0);
        }
        let player_pos = player.get_entity().pos.load();
        let hooked_pos = hooked.get_entity().pos.load();
        let delta = player_pos - hooked_pos;
        let motion = delta
            .multiply(0.1, 0.1, 0.1)
            .add_raw(0.0, delta.length().sqrt() * 0.08, 0.0);
        hooked.get_entity().add_velocity(motion);
    }

    fn grant_experience(player: &Player, world: &World, points: i32) {
        if points <= 0 {
            return;
        }
        let Some(player_arc) = world.get_player_by_uuid(player.gameprofile.id) else {
            return;
        };
        let remaining = player_arc.apply_mending_from_xp(points);
        if remaining > 0 {
            player_arc.add_experience_points(remaining);
        }
    }

    fn stick_to_hooked(&self, world: &World, entity: &Entity) -> bool {
        let hooked_id = self.hooked_entity_id.load(Ordering::Relaxed);
        if hooked_id == 0 {
            return false;
        }
        let Some(hooked) = world.get_entity_by_id(hooked_id) else {
            self.hooked_entity_id.store(0, Ordering::Relaxed);
            return false;
        };
        if hooked.get_entity().removed.load(Ordering::Relaxed) {
            self.hooked_entity_id.store(0, Ordering::Relaxed);
            return false;
        }
        let mut hooked_pos = hooked.get_entity().pos.load();
        hooked_pos.y += hooked.get_entity().get_eye_height() * 0.8;
        entity.set_pos(hooked_pos);
        true
    }

    fn try_hook(&self, entity: &Entity, world: &World, search_box: &BoundingBox) {
        if self.hooked_entity_id.load(Ordering::Relaxed) != 0 {
            return;
        }
        let candidates = world.get_entities_at_box(search_box);
        for cand in candidates {
            if cand.get_entity().entity_id == self.owner_id
                || cand.get_entity().entity_id == entity.entity_id
            {
                continue;
            }
            if is_projectile(cand.get_entity().entity_type) {
                continue;
            }
            let ebb = cand.get_entity().bounding_box.load().expand(0.3, 0.3, 0.3);
            if ebb.intersects(search_box) {
                let hooked_id = cand.get_entity().entity_id;
                self.hooked_entity_id.store(hooked_id, Ordering::Relaxed);
                entity.set_synced_data(
                    pumpkin_data::tracked_data::fishing_bobber::HOOKED_ENTITY,
                    hooked_id + 1,
                );
                return;
            }
        }
    }

    fn clear_bite(&self, entity: &Entity) {
        if self.bite_countdown.swap(0, Ordering::Relaxed) > 0 {
            entity.set_synced_data(
                pumpkin_data::tracked_data::fishing_bobber::DATA_BITING,
                false,
            );
        }
    }

    fn tick_bite(&self, entity: &Entity, world: &World, open_water: bool) {
        if !open_water {
            self.clear_bite(entity);
            return;
        }

        let bite = self.bite_countdown.load(Ordering::Relaxed);
        if bite > 0 {
            let next = bite - 1;
            self.bite_countdown.store(next, Ordering::Relaxed);
            if next == 0 {
                entity.set_synced_data(
                    pumpkin_data::tracked_data::fishing_bobber::DATA_BITING,
                    false,
                );
            } else if bite % 5 == 0 {
                world.spawn_particle(
                    entity.pos.load(),
                    Vector3::new(0.1f32, 0.1f32, 0.1f32),
                    0.0,
                    5,
                    pumpkin_data::particle::Particle::Bubble,
                );
            }
            return;
        }

        let wait = self.wait_countdown.load(Ordering::Relaxed);
        if wait > 0 {
            self.wait_countdown.store(wait - 1, Ordering::Relaxed);
            return;
        }

        self.bite_countdown
            .store(Self::BITE_WINDOW, Ordering::Relaxed);
        self.wait_countdown.store(random_wait(), Ordering::Relaxed);
        entity.set_synced_data(
            pumpkin_data::tracked_data::fishing_bobber::DATA_BITING,
            true,
        );
        world.play_sound(
            Sound::EntityFishingBobberSplash,
            SoundCategory::Neutral,
            &entity.pos.load(),
        );
    }

    pub fn process_tick(&self, caller: &dyn EntityBase) {
        let entity = self.get_entity();
        let world = entity.world.load();

        if self.in_ground.load(Ordering::Relaxed) {
            return;
        }
        if self.stick_to_hooked(&world, entity) {
            return;
        }

        let start_pos = entity.pos.load();
        let mut velocity = entity.velocity.load();
        let in_water = block_is_water(
            &world,
            BlockPos::floored(start_pos.x, start_pos.y, start_pos.z),
        );
        entity.touching_water.store(in_water, Ordering::Relaxed);

        if in_water {
            let above = BlockPos::floored(start_pos.x, start_pos.y + 1.0, start_pos.z);
            if block_is_water(&world, above) {
                velocity.y += 0.08;
            } else {
                velocity.y = 0.0;
            }
        } else {
            velocity.y -= Self::GRAVITY;
        }

        let inertia = if in_water {
            Self::WATER_INERTIA
        } else {
            Self::AIR_INERTIA
        };
        velocity = velocity.multiply(inertia, inertia, inertia);

        let mut new_pos = start_pos.add(&velocity);
        let mut landed_in_water = in_water;
        if !in_water && let Some(water_point) = first_water_along(&world, start_pos, new_pos) {
            new_pos = snap_into_water(water_point);
            velocity = Vector3::new(velocity.x * 0.05, 0.0, velocity.z * 0.05);
            landed_in_water = true;
            entity.touching_water.store(true, Ordering::Relaxed);
        }

        if landed_in_water {
            new_pos = float_on_surface(&world, new_pos);
        } else {
            let search_box = segment_box(start_pos, new_pos);
            let (block_cols, _) = world.get_block_collisions(search_box, caller);
            if !block_cols.is_empty() {
                self.in_ground.store(true, Ordering::Relaxed);
                entity.velocity.store(Vector3::new(0.0, 0.0, 0.0));
                self.clear_bite(entity);
                return;
            }
        }

        entity.velocity.store(velocity);
        entity.set_pos(new_pos);
        self.try_hook(entity, &world, &segment_box(start_pos, new_pos));

        if landed_in_water {
            let open = position_is_open_water(&world, entity.pos.load());
            self.tick_bite(entity, &world, open);
        } else {
            self.clear_bite(entity);
        }
    }
}

impl EntityBase for FishingBobberEntity {
    fn get_owner_id(&self) -> Option<i32> {
        Some(self.owner_id)
    }

    fn get_entity(&self) -> &Entity {
        &self.entity
    }

    fn get_living_entity(&self) -> Option<&LivingEntity> {
        None
    }

    fn cast_any(&self) -> &dyn std::any::Any {
        self
    }
    fn on_hit(&self, _hit: ProjectileHit) {
        self.has_hit.store(true, Ordering::Relaxed);
    }

    fn tick(&self, caller: &dyn EntityBase, _server: &Server) {
        self.process_tick(caller);
    }
}

#[cfg(test)]
mod tests {
    use super::{FishingLayer, fishing_catch_plan, layers_are_open_water};

    #[test]
    fn bite_in_open_water_rolls_loot_and_costs_the_rod() {
        let plan = fishing_catch_plan(false, true, true, 3);
        assert!(plan.roll_loot);
        assert!(!plan.pull_hooked);
        assert_eq!(plan.rod_damage, 1);
        assert_eq!(plan.experience, 3);
    }

    #[test]
    fn hooked_catch_pulls_instead_of_rolling_loot() {
        let plan = fishing_catch_plan(true, true, true, 4);
        assert!(plan.pull_hooked);
        assert!(!plan.roll_loot);
        assert_eq!(plan.rod_damage, 0);
        assert_eq!(plan.experience, 0);
    }

    #[test]
    fn bite_outside_open_water_catches_nothing() {
        let plan = fishing_catch_plan(false, true, false, 2);
        assert!(!plan.roll_loot);
        assert!(!plan.pull_hooked);
        assert_eq!(plan.rod_damage, 0);
        assert_eq!(plan.experience, 0);
    }

    #[test]
    fn waiting_bobber_catches_nothing() {
        let plan = fishing_catch_plan(false, false, true, 5);
        assert!(!plan.roll_loot);
        assert_eq!(plan.experience, 0);
        assert_eq!(plan.rod_damage, 0);
    }

    #[test]
    fn experience_roll_stays_small() {
        assert_eq!(fishing_catch_plan(false, true, true, 0).experience, 1);
        assert_eq!(fishing_catch_plan(false, true, true, 1).experience, 1);
        assert_eq!(fishing_catch_plan(false, true, true, 6).experience, 6);
        assert_eq!(fishing_catch_plan(false, true, true, 99).experience, 6);
    }

    #[test]
    fn open_water_is_water_with_air_above() {
        assert!(layers_are_open_water(&[
            FishingLayer::Water,
            FishingLayer::Water,
            FishingLayer::Above,
            FishingLayer::Above,
        ]));
        assert!(layers_are_open_water(&[
            FishingLayer::Water,
            FishingLayer::Water,
            FishingLayer::Water,
            FishingLayer::Above,
        ]));
    }

    #[test]
    fn blocked_or_air_only_columns_are_not_open_water() {
        assert!(!layers_are_open_water(&[
            FishingLayer::Blocked,
            FishingLayer::Water,
            FishingLayer::Above,
            FishingLayer::Above,
        ]));
        assert!(!layers_are_open_water(&[
            FishingLayer::Above,
            FishingLayer::Above,
            FishingLayer::Above,
            FishingLayer::Above,
        ]));
        assert!(!layers_are_open_water(&[
            FishingLayer::Water,
            FishingLayer::Above,
            FishingLayer::Water,
            FishingLayer::Above,
        ]));
        assert!(!layers_are_open_water(&[
            FishingLayer::Water,
            FishingLayer::Water,
            FishingLayer::Water,
            FishingLayer::Water,
        ]));
    }

    #[test]
    fn fishing_loot_table_yields_an_item() {
        for seed in [1i64, 2, 42, 1000] {
            let items = crate::world::loot::generate_loot(
                &pumpkin_data::loot_table::GAMEPLAY_FISHING,
                seed,
            );
            assert!(!items.is_empty(), "fishing loot was empty for seed {seed}");
            assert!(items.iter().all(|stack| !stack.is_empty()));
        }
    }
}
