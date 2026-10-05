use std::sync::{
    Arc,
    atomic::{AtomicI32, AtomicU32, Ordering},
};

use pumpkin_data::entity::EntityType;
use pumpkin_util::math::vector3::Vector3;

use crate::{server::Server, world::World};

use super::{Entity, EntityBase, living::LivingEntity, player::Player};

const PICKUP_COOLDOWN: u32 = 2;
const DESPAWN_AGE: u32 = 6000;
const TARGET_SCAN_INTERVAL: u32 = 20;
const FOLLOW_RANGE: f64 = 7.25;
const FOLLOW_PULL: f64 = 0.1;
const WATER_DRAG: f64 = 0.99;
const WATER_BUOYANCY: f64 = 5.0e-4;
const WATER_RISE_LIMIT: f64 = 0.06;
const WATER_HEIGHT_FOR_BUOYANCY: f64 = 0.1;
const MOTION_DRAG: f64 = 0.98;

pub struct ExperienceOrbEntity {
    entity: Entity,
    amount: u32,
    orb_age: AtomicU32,
    pickup_delay: AtomicU32,
    target_id: AtomicI32,
}

impl ExperienceOrbEntity {
    pub fn new(entity: Entity, amount: u32) -> Self {
        entity.yaw.store(rand::random::<f32>() * 360.0);
        Self {
            entity,
            amount,
            orb_age: AtomicU32::new(0),
            pickup_delay: AtomicU32::new(0),
            target_id: AtomicI32::new(0),
        }
    }

    pub fn spawn(world: &Arc<World>, position: Vector3<f64>, amount: u32) {
        let mut amount = amount;
        while amount > 0 {
            let i = Self::round_to_orb_size(amount);
            amount -= i;
            let entity = Entity::new(world.clone(), position, &EntityType::EXPERIENCE_ORB);
            let orb = Arc::new(Self::new(entity, i));
            world.spawn_entity(orb);
        }
    }

    const fn round_to_orb_size(value: u32) -> u32 {
        if value >= 2477 {
            2477
        } else if value >= 1237 {
            1237
        } else if value >= 617 {
            617
        } else if value >= 307 {
            307
        } else if value >= 149 {
            149
        } else if value >= 73 {
            73
        } else if value >= 37 {
            37
        } else if value >= 17 {
            17
        } else if value >= 7 {
            7
        } else if value >= 3 {
            3
        } else {
            1
        }
    }

    fn refresh_target(&self) {
        let center = orb_center(self.entity.pos.load());
        let player = self
            .entity
            .world
            .load()
            .get_nearest_player(center, FOLLOW_RANGE, |player| {
                player.living_entity.health.load() > 0.0 && !player.is_spectator()
            });
        let id = player.map_or(0, |player| player.entity_id());
        self.target_id.store(id, Ordering::Relaxed);
    }

    fn tracked_player(&self) -> Option<Arc<Player>> {
        let id = self.target_id.load(Ordering::Relaxed);
        if id == 0 {
            return None;
        }
        let player = self.entity.world.load().get_player_by_id(id)?;
        if player.living_entity.health.load() <= 0.0 || player.is_spectator() {
            self.target_id.store(0, Ordering::Relaxed);
            return None;
        }
        Some(player)
    }

    fn absorb(&self, player: &Arc<Player>) {
        if self.entity.removed.load(Ordering::Relaxed) {
            return;
        }
        if player.living_entity.health.load() <= 0.0 || player.is_spectator() {
            return;
        }

        let mut player_delay = player
            .experience_pick_up_delay
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !can_absorb(self.pickup_delay.load(Ordering::Relaxed)) || !can_absorb(*player_delay) {
            return;
        }
        *player_delay = PICKUP_COOLDOWN;
        drop(player_delay);

        player.living_entity.pickup(&self.entity, 1);
        self.entity.remove();

        let remaining = player.apply_mending_from_xp(points_from_amount(self.amount));
        if remaining > 0 {
            grant_experience(player, remaining);
        }
    }
}

fn orb_center(pos: Vector3<f64>) -> Vector3<f64> {
    let half_height = f64::from(EntityType::EXPERIENCE_ORB.dimension[1]) * 0.5;
    Vector3::new(pos.x, pos.y + half_height, pos.z)
}

fn follow_aim(feet: Vector3<f64>, eye_height: f64) -> Vector3<f64> {
    Vector3::new(feet.x, feet.y + eye_height * 0.5, feet.z)
}

fn water_buoyancy_step(
    in_water: bool,
    water_height: f64,
    mut velocity: Vector3<f64>,
    gravity: f64,
    no_physics: bool,
) -> Vector3<f64> {
    if in_water && water_height > WATER_HEIGHT_FOR_BUOYANCY {
        velocity.x *= WATER_DRAG;
        velocity.z *= WATER_DRAG;
        if velocity.y < WATER_RISE_LIMIT {
            velocity.y += WATER_BUOYANCY;
        }
        return velocity;
    }
    if !no_physics {
        velocity.y -= gravity;
    }
    velocity
}

fn follow_step(
    orb_pos: Vector3<f64>,
    feet: Vector3<f64>,
    aim: Vector3<f64>,
    velocity: Vector3<f64>,
) -> Vector3<f64> {
    let center = orb_center(orb_pos);
    let offset = feet.sub(&center);
    let distance_squared = offset.length_squared();
    if distance_squared >= FOLLOW_RANGE * FOLLOW_RANGE || distance_squared <= 1.0e-7 {
        return velocity;
    }
    let closeness = 1.0 - distance_squared.sqrt() / FOLLOW_RANGE;
    let pull = closeness * closeness * FOLLOW_PULL;
    let direction = aim.sub(&center).normalize();
    velocity.add(&direction.multiply(pull, pull, pull))
}

fn drag_velocity(mut velocity: Vector3<f64>) -> Vector3<f64> {
    velocity.x *= MOTION_DRAG;
    velocity.y *= MOTION_DRAG;
    velocity.z *= MOTION_DRAG;
    velocity
}

const fn can_absorb(pickup_delay: u32) -> bool {
    pickup_delay == 0
}

const fn next_pickup_delay(pickup_delay: u32) -> u32 {
    pickup_delay.saturating_sub(1)
}

const fn saturating_experience(current: i32, added: i32) -> i32 {
    current.saturating_add(added)
}

fn points_from_amount(amount: u32) -> i32 {
    i32::try_from(amount).unwrap_or(i32::MAX)
}

fn player_experience_total(player: &Player) -> i32 {
    pumpkin_util::math::experience::points_to_level(player.get_experience_level())
        .saturating_add(player.get_total_experience())
}

fn grant_experience(player: &Arc<Player>, amount: i32) {
    if amount <= 0 {
        return;
    }
    let current = player_experience_total(player);
    let updated = saturating_experience(current, amount);
    let delta = updated.saturating_sub(current);
    if delta > 0 {
        player.add_experience_points(delta);
    }
}

impl EntityBase for ExperienceOrbEntity {
    fn tick(&self, caller: &dyn EntityBase, server: &Server) {
        let entity = &self.entity;
        entity.tick(caller, server);

        self.pickup_delay.store(
            next_pickup_delay(self.pickup_delay.load(Ordering::Relaxed)),
            Ordering::Relaxed,
        );

        if self
            .orb_age
            .load(Ordering::Relaxed)
            .is_multiple_of(TARGET_SCAN_INTERVAL)
        {
            self.refresh_target();
        }

        let bounding_box = entity.bounding_box.load();
        let no_physics = !entity
            .world
            .load()
            .is_space_empty(bounding_box.expand(-1.0e-7, -1.0e-7, -1.0e-7));
        entity.no_physics.store(no_physics, Ordering::Relaxed);

        let mut velocity = water_buoyancy_step(
            entity.touching_water.load(Ordering::SeqCst),
            entity.water_height.load(),
            entity.velocity.load(),
            self.get_gravity(),
            no_physics,
        );
        if let Some(player) = self.tracked_player() {
            let feet = player.position();
            let aim = follow_aim(feet, player.living_entity.entity.get_eye_height());
            velocity = follow_step(entity.pos.load(), feet, aim, velocity);
        }

        entity.velocity.store(velocity);
        entity.move_entity(caller, velocity);
        entity.velocity.store(drag_velocity(entity.velocity.load()));
        entity.tick_block_collisions(caller);

        if let Some(player) = self.tracked_player()
            && player
                .get_entity()
                .bounding_box
                .load()
                .intersects(&entity.bounding_box.load())
        {
            self.absorb(&player);
        }

        let age = self.orb_age.fetch_add(1, Ordering::Relaxed);
        if age >= DESPAWN_AGE {
            entity.remove();
        }
    }

    fn get_entity(&self) -> &Entity {
        &self.entity
    }

    fn on_player_collision(&self, player: &Arc<Player>) {
        self.absorb(player);
    }

    fn get_living_entity(&self) -> Option<&LivingEntity> {
        None
    }

    fn get_gravity(&self) -> f64 {
        0.03
    }

    fn cast_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{
        PICKUP_COOLDOWN, can_absorb, next_pickup_delay, saturating_experience, water_buoyancy_step,
    };
    use pumpkin_util::math::vector3::Vector3;

    #[test]
    fn water_buoyancy_drifts_up_instead_of_sinking() {
        let gravity = 0.03;
        let sinking = Vector3::new(2.0, -0.2, -4.0);

        let in_water = water_buoyancy_step(true, 0.5, sinking, gravity, false);
        assert!(in_water.y > sinking.y);
        assert!((in_water.x - sinking.x * 0.99).abs() < 1.0e-9);
        assert!((in_water.z - sinking.z * 0.99).abs() < 1.0e-9);

        let resting = water_buoyancy_step(true, 0.5, Vector3::new(0.0, 0.0, 0.0), gravity, false);
        assert!(resting.y > 0.0);

        let capped = water_buoyancy_step(true, 1.0, Vector3::new(0.0, 0.06, 0.0), gravity, false);
        assert!((capped.y - 0.06).abs() < 1.0e-9);

        let dry = water_buoyancy_step(false, 0.0, sinking, gravity, false);
        assert!((dry.y - (sinking.y - gravity)).abs() < 1.0e-9);

        let shallow = water_buoyancy_step(true, 0.1, Vector3::new(0.0, 0.0, 0.0), gravity, false);
        assert!((shallow.y + gravity).abs() < 1.0e-9);
    }

    #[test]
    fn pickup_delay_blocks_absorb_until_it_expires() {
        assert!(!can_absorb(PICKUP_COOLDOWN));
        let mut delay = PICKUP_COOLDOWN;
        let mut ticks = 0_u32;
        while !can_absorb(delay) {
            delay = next_pickup_delay(delay);
            ticks += 1;
        }
        assert_eq!(ticks, PICKUP_COOLDOWN);
        assert_eq!(delay, 0);
        assert!(can_absorb(delay));
        assert_eq!(next_pickup_delay(0), 0);
    }

    #[test]
    fn experience_add_saturates_at_signed_max() {
        assert_eq!(saturating_experience(20, 22), 42);
        assert_eq!(saturating_experience(i32::MAX - 4, 4), i32::MAX);
        assert_eq!(saturating_experience(i32::MAX - 4, 5), i32::MAX);
        assert_eq!(saturating_experience(i32::MAX, 1), i32::MAX);
        assert_eq!(saturating_experience(i32::MAX, i32::MAX), i32::MAX);
        assert!(saturating_experience(i32::MAX, 1) > 0);
    }
}
