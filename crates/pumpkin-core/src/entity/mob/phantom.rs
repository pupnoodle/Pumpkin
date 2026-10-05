use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU8, Ordering};
use std::sync::{Arc, Weak};

use crossbeam::atomic::AtomicCell;
use pumpkin_data::attributes::Attributes;
use pumpkin_data::entity::EntityType;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_util::GameMode;
use pumpkin_util::math::vector3::Vector3;
use rand::RngExt;
use uuid::Uuid;

use crate::entity::player::Player;
use crate::entity::player::statistics::CustomStatistic;
use crate::entity::{
    Entity, EntityBase,
    ai::goal::{look_around::RandomLookAroundGoal, look_at_entity::LookAtEntityGoal},
    mob::{Mob, MobEntity},
};
use crate::world::World;

const INSOMNIA_TICKS: i32 = 72_000;
const ANCHOR_MIN_HEIGHT: f64 = 20.0;
const ANCHOR_HEIGHT_SPAN: f64 = 20.0;
const ORBIT_RADIUS_MIN: f64 = 5.0;
const ORBIT_RADIUS_MAX: f64 = 15.0;
const ORBIT_ANGLE_STEP: f64 = 0.05;
const ORBIT_CATCH_DISTANCE_SQ: f64 = 9.0;
const SWOOP_COOLDOWN_BASE: i32 = 160;
const SWOOP_COOLDOWN_JITTER: i32 = 80;
const SWOOP_OVERSHOOT_DISTANCE_SQ: f64 = 16.0;
const CIRCLE_SPEED_FACTOR: f64 = 0.5;
const PHASE_CIRCLE: u8 = 0;
const PHASE_SWOOP: u8 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PhantomAttackPhase {
    Circle,
    Swoop,
}

const fn phantom_preys_on(survival_or_adventure: bool, alive: bool, time_since_rest: i32) -> bool {
    survival_or_adventure && alive && time_since_rest >= INSOMNIA_TICKS
}

const fn prey_search_range(follow_range: f64) -> f64 {
    follow_range + ANCHOR_MIN_HEIGHT + ANCHOR_HEIGHT_SPAN
}

fn anchor_above(player_pos: Vector3<f64>, extra_height: f64, sea_level: i32) -> Vector3<f64> {
    let y = (player_pos.y + ANCHOR_MIN_HEIGHT + extra_height).max(f64::from(sea_level) + 1.0);
    Vector3::new(player_pos.x, y, player_pos.z)
}

fn orbit_point(anchor: Vector3<f64>, angle: f64, radius: f64) -> Vector3<f64> {
    Vector3::new(
        anchor.x + angle.cos() * radius,
        anchor.y,
        anchor.z + angle.sin() * radius,
    )
}

fn flight_move_target(
    phase: PhantomAttackPhase,
    orbit: Vector3<f64>,
    player_pos: Vector3<f64>,
) -> Vector3<f64> {
    match phase {
        PhantomAttackPhase::Circle => orbit,
        PhantomAttackPhase::Swoop => player_pos,
    }
}

fn swoop_overshot(phantom_y: f64, player_y: f64, horizontal_dist_sq: f64) -> bool {
    phantom_y < player_y && horizontal_dist_sq <= SWOOP_OVERSHOOT_DISTANCE_SQ
}

fn step_phantom_phase(
    phase: PhantomAttackPhase,
    cooldown: i32,
    touched_player: bool,
    overshot: bool,
    cooldown_after_swoop: i32,
) -> (PhantomAttackPhase, i32) {
    match phase {
        PhantomAttackPhase::Circle if cooldown <= 0 => (PhantomAttackPhase::Swoop, 0),
        PhantomAttackPhase::Circle => (PhantomAttackPhase::Circle, cooldown - 1),
        PhantomAttackPhase::Swoop if touched_player || overshot => {
            (PhantomAttackPhase::Circle, cooldown_after_swoop)
        }
        PhantomAttackPhase::Swoop => (PhantomAttackPhase::Swoop, cooldown),
    }
}

fn steer_toward(pos: Vector3<f64>, target: Vector3<f64>, speed: f64) -> Vector3<f64> {
    let delta = target.sub(&pos);
    let dist = delta.length();
    if dist < 1.0e-4 {
        return Vector3::new(0.0, 0.0, 0.0);
    }
    let scale = speed.min(dist);
    let dir = delta.normalize();
    Vector3::new(dir.x * scale, dir.y * scale, dir.z * scale)
}

fn player_is_phantom_prey(player: &Player) -> bool {
    let mode = player.gamemode.load();
    let survival_or_adventure = matches!(mode, GameMode::Survival | GameMode::Adventure);
    let living = &player.living_entity;
    let alive = player.get_entity().is_alive()
        && living.health.load() > 0.0
        && !living.dead.load(Ordering::Relaxed);
    phantom_preys_on(
        survival_or_adventure,
        alive,
        player.get_custom_stat(CustomStatistic::TimeSinceRest),
    )
}

pub struct PhantomEntity {
    pub mob_entity: MobEntity,
    phase: AtomicU8,
    swoop_cooldown: AtomicI32,
    anchor: AtomicCell<Vector3<f64>>,
    anchor_set: AtomicBool,
    orbit_angle: AtomicCell<f64>,
    orbit_radius: AtomicCell<f64>,
    orbit_clockwise: AtomicBool,
    prey: AtomicCell<Option<Uuid>>,
}

impl PhantomEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let phantom = Self {
            mob_entity,
            phase: AtomicU8::new(PHASE_CIRCLE),
            swoop_cooldown: AtomicI32::new(SWOOP_COOLDOWN_BASE),
            anchor: AtomicCell::new(Vector3::new(0.0, 0.0, 0.0)),
            anchor_set: AtomicBool::new(false),
            orbit_angle: AtomicCell::new(0.0),
            orbit_radius: AtomicCell::new(10.0),
            orbit_clockwise: AtomicBool::new(true),
            prey: AtomicCell::new(None),
        };
        let mob_arc = Arc::new(phantom);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        {
            let mut goal_selector = mob_arc
                .mob_entity
                .goals_selector
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            goal_selector.add_goal(
                6,
                LookAtEntityGoal::with_default(mob_weak, &EntityType::PLAYER, 8.0),
            );
            goal_selector.add_goal(6, Box::new(RandomLookAroundGoal::default()));
        };

        mob_arc
    }

    fn attack_phase(&self) -> PhantomAttackPhase {
        if self.phase.load(Ordering::Relaxed) == PHASE_SWOOP {
            PhantomAttackPhase::Swoop
        } else {
            PhantomAttackPhase::Circle
        }
    }

    fn set_attack_phase(&self, phase: PhantomAttackPhase) {
        let raw = match phase {
            PhantomAttackPhase::Circle => PHASE_CIRCLE,
            PhantomAttackPhase::Swoop => PHASE_SWOOP,
        };
        self.phase.store(raw, Ordering::Relaxed);
    }

    fn acquire_prey(&self, world: &World, pos: Vector3<f64>) -> Option<Arc<Player>> {
        let follow = self
            .mob_entity
            .living_entity
            .get_attribute_value(&Attributes::FOLLOW_RANGE);
        let search = prey_search_range(follow);
        let keep_sq = search * search;
        if let Some(id) = self.prey.load()
            && let Some(player) = world.get_player_by_uuid(id)
            && player_is_phantom_prey(player.as_ref())
            && pos.squared_distance_to_vec_xz(player.get_entity().pos.load()) <= keep_sq
        {
            return Some(player);
        }

        let found = world.get_nearest_player(pos, search, |player| {
            player_is_phantom_prey(player.as_ref())
        });
        let found_id = found.as_ref().map(|player| player.gameprofile.id);
        if found_id != self.prey.load() {
            self.anchor_set.store(false, Ordering::Relaxed);
        }
        self.prey.store(found_id);
        found
    }

    fn pick_anchor(&self, player_pos: Vector3<f64>, sea_level: i32) {
        let (extra, radius, clockwise, angle) = {
            let mut rng = self.get_random();
            (
                rng.random_range(0.0..ANCHOR_HEIGHT_SPAN),
                rng.random_range(ORBIT_RADIUS_MIN..ORBIT_RADIUS_MAX),
                rng.random_range(0..2) == 0,
                rng.random_range(0.0..std::f64::consts::TAU),
            )
        };
        self.anchor
            .store(anchor_above(player_pos, extra, sea_level));
        self.orbit_radius.store(radius);
        self.orbit_clockwise.store(clockwise, Ordering::Relaxed);
        self.orbit_angle.store(angle);
        self.anchor_set.store(true, Ordering::Relaxed);
    }

    fn tick_flight(&self, caller: &dyn EntityBase) {
        let entity = &self.mob_entity.living_entity.entity;
        let living = &self.mob_entity.living_entity;
        if !entity.is_alive() || living.health.load() <= 0.0 || living.dead.load(Ordering::Relaxed)
        {
            return;
        }

        let world = entity.world.load();
        let pos = entity.pos.load();
        let Some(player) = self.acquire_prey(&world, pos) else {
            self.anchor_set.store(false, Ordering::Relaxed);
            self.set_attack_phase(PhantomAttackPhase::Circle);
            self.swoop_cooldown
                .store(SWOOP_COOLDOWN_BASE, Ordering::Relaxed);
            entity.velocity.store(Vector3::new(0.0, 0.0, 0.0));
            return;
        };

        let player_pos = player.get_entity().pos.load();
        let phase = self.attack_phase();
        let swooping = phase == PhantomAttackPhase::Swoop;
        let touched = swooping
            && entity
                .bounding_box
                .load()
                .expand(0.2, 0.5, 0.2)
                .intersects(&player.get_entity().bounding_box.load());
        let overshot = swooping
            && swoop_overshot(
                pos.y,
                player_pos.y,
                pos.squared_distance_to_vec_xz(player_pos),
            );
        if touched {
            self.mob_entity.try_attack(caller, player.as_ref());
            world.play_sound(Sound::EntityPhantomBite, SoundCategory::Hostile, &pos);
        }

        let cooldown_after_swoop = if swooping && (touched || overshot) {
            SWOOP_COOLDOWN_BASE + self.get_random().random_range(0..SWOOP_COOLDOWN_JITTER)
        } else {
            SWOOP_COOLDOWN_BASE
        };
        let (next_phase, next_cooldown) = step_phantom_phase(
            phase,
            self.swoop_cooldown.load(Ordering::Relaxed),
            touched,
            overshot,
            cooldown_after_swoop,
        );
        if phase == PhantomAttackPhase::Circle && next_phase == PhantomAttackPhase::Swoop {
            world.play_sound(Sound::EntityPhantomSwoop, SoundCategory::Hostile, &pos);
        }
        if !self.anchor_set.load(Ordering::Relaxed)
            || (next_phase == PhantomAttackPhase::Circle && swooping)
        {
            self.pick_anchor(player_pos, world.sea_level);
        }

        let mut angle = self.orbit_angle.load();
        let radius = self.orbit_radius.load();
        let mut orbit = orbit_point(self.anchor.load(), angle, radius);
        if next_phase == PhantomAttackPhase::Circle {
            let close = pos.squared_distance_to_vec(&orbit) < ORBIT_CATCH_DISTANCE_SQ;
            let step = if self.orbit_clockwise.load(Ordering::Relaxed) {
                ORBIT_ANGLE_STEP
            } else {
                -ORBIT_ANGLE_STEP
            };
            if close {
                angle += step;
                self.orbit_angle.store(angle);
                orbit = orbit_point(self.anchor.load(), angle, radius);
            }
        }

        let move_target = flight_move_target(next_phase, orbit, player_pos);
        let movement_speed = living.get_attribute_value(&Attributes::MOVEMENT_SPEED);
        let speed = if next_phase == PhantomAttackPhase::Swoop {
            movement_speed
        } else {
            movement_speed * CIRCLE_SPEED_FACTOR
        };
        let velocity = steer_toward(pos, move_target, speed);
        entity.velocity.store(velocity);
        if velocity.horizontal_length_squared() > 1.0e-6 {
            let yaw = (velocity.z.atan2(velocity.x) as f32).to_degrees() - 90.0;
            entity.yaw.store(yaw);
            entity.head_yaw.store(yaw);
            entity.body_yaw.store(yaw);
            let pitch = (-velocity.y).atan2(velocity.horizontal_length()) as f32;
            entity.pitch.store(pitch.to_degrees());
        }

        self.set_attack_phase(next_phase);
        self.swoop_cooldown.store(next_cooldown, Ordering::Relaxed);
    }
}

impl Mob for PhantomEntity {
    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn get_mob_gravity(&self) -> f64 {
        0.0
    }

    fn custom_server_ai_step(&self, caller: &dyn EntityBase) {
        self.tick_flight(caller);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        INSOMNIA_TICKS, PhantomAttackPhase, anchor_above, flight_move_target, orbit_point,
        phantom_preys_on, step_phantom_phase, swoop_overshot,
    };
    use pumpkin_util::math::vector3::Vector3;

    #[test]
    fn orbits_above_then_swoops_and_returns() {
        let player = Vector3::new(0.0, 64.0, 0.0);
        let anchor = anchor_above(player, 0.0, 63);
        assert!(anchor.y >= player.y + 20.0);
        let orbit = orbit_point(anchor, 0.0, 10.0);
        let horizontal = ((orbit.x - anchor.x).powi(2) + (orbit.z - anchor.z).powi(2)).sqrt();
        assert!((horizontal - 10.0).abs() < 1.0e-6);
        assert_eq!(orbit.y, anchor.y);
        assert_eq!(
            flight_move_target(PhantomAttackPhase::Circle, orbit, player),
            orbit
        );
        assert_eq!(
            flight_move_target(PhantomAttackPhase::Swoop, orbit, player),
            player
        );

        let (phase, cooldown) =
            step_phantom_phase(PhantomAttackPhase::Circle, 2, false, false, 160);
        assert_eq!(phase, PhantomAttackPhase::Circle);
        assert_eq!(cooldown, 1);
        let (phase, _) = step_phantom_phase(PhantomAttackPhase::Circle, 0, false, false, 160);
        assert_eq!(phase, PhantomAttackPhase::Swoop);
        let (phase, cooldown) = step_phantom_phase(phase, 0, true, false, 160);
        assert_eq!(phase, PhantomAttackPhase::Circle);
        assert_eq!(cooldown, 160);
        let (phase, _) = step_phantom_phase(PhantomAttackPhase::Swoop, 0, false, false, 160);
        assert_eq!(phase, PhantomAttackPhase::Swoop);
        assert!(swoop_overshot(60.0, 64.0, 4.0));
        let (phase, _) = step_phantom_phase(PhantomAttackPhase::Swoop, 0, false, true, 160);
        assert_eq!(phase, PhantomAttackPhase::Circle);

        assert!(phantom_preys_on(true, true, INSOMNIA_TICKS));
        assert!(!phantom_preys_on(true, true, INSOMNIA_TICKS - 1));
        assert!(!phantom_preys_on(false, true, INSOMNIA_TICKS));
        assert!(!phantom_preys_on(true, false, INSOMNIA_TICKS));
    }
}
