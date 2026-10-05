use crate::entity::mob::equipment::RegionalDifficulty;
use crate::entity::mob::zombie::ZombieEntityBase;
use crate::entity::mob::{Mob, MobEntity};
use crate::entity::{Entity, EntityBase};
use crate::world::World;
use pumpkin_data::entity::EntityType;
use pumpkin_nbt::compound::NbtCompound;
use std::sync::Arc;
use std::sync::atomic::Ordering;

pub struct ZombieEntity {
    entity: Arc<ZombieEntityBase>,
}

impl ZombieEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let entity = ZombieEntityBase::new(entity);
        let zombie = Self { entity };
        Arc::new(zombie)
    }

    #[must_use]
    pub fn with_can_break_doors(entity: Entity, can_break_doors: bool) -> Arc<Self> {
        let entity = ZombieEntityBase::with_can_break_doors(entity, can_break_doors);
        let zombie = Self { entity };
        Arc::new(zombie)
    }
}

impl Mob for ZombieEntity {
    fn get_mob_entity(&self) -> &MobEntity {
        &self.entity.mob_entity
    }

    fn spawn_as_baby(&self) -> bool {
        self.entity.spawn_as_baby()
    }

    fn populate_default_equipment_slots(
        &self,
        world: &Arc<World>,
        difficulty: &RegionalDifficulty,
    ) {
        self.entity
            .populate_default_equipment_slots(world, difficulty);
    }

    fn populate_default_equipment_enchantments(&self, difficulty: &RegionalDifficulty) {
        self.entity
            .populate_default_equipment_enchantments(difficulty);
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        self.entity.mob_write_nbt(nbt);
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        self.entity.mob_read_nbt(nbt);
    }

    fn mob_tick(&self, _caller: &dyn EntityBase) {
        let living = &self.entity.mob_entity.living_entity;
        if living.is_alive() && living.entity.is_under_water() {
            self.convert_to_drowned();
        }
    }
}

impl ZombieEntity {
    #[must_use]
    pub fn can_break_doors(&self) -> bool {
        self.entity
            .can_break_doors
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn set_can_break_doors(&self, can_break: bool) {
        self.entity
            .can_break_doors
            .store(can_break, std::sync::atomic::Ordering::Relaxed);
    }

    #[must_use]
    pub fn is_baby(&self) -> bool {
        self.entity.is_baby()
    }

    pub fn set_baby(&self, baby: bool) {
        self.entity.set_baby(baby);
    }

    /// Vanilla `Zombie.tick`: replace the zombie with the drowned it
    /// becomes when it goes underwater.
    fn convert_to_drowned(&self) {
        let entity = &self.entity.mob_entity.living_entity.entity;
        let world = entity.world.load();
        let pos = entity.pos.load();

        let drowned = crate::entity::r#type::from_type(
            &EntityType::DROWNED,
            pos,
            &world,
            uuid::Uuid::new_v4(),
        );
        let drowned_entity = drowned.get_entity();
        drowned_entity.set_rotation(entity.yaw.load(), entity.pitch.load());
        drowned_entity.head_yaw.store(entity.head_yaw.load());
        drowned_entity.velocity.store(entity.velocity.load());
        drowned_entity.invulnerable.store(
            entity.invulnerable.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
        if let Some(custom_name) = &**entity.custom_name.load() {
            drowned_entity.set_custom_name(custom_name.clone());
            drowned_entity
                .set_custom_name_visible(entity.custom_name_visible.load(Ordering::Relaxed));
        }

        if let Some(mob) = drowned.get_mob() {
            let mob_entity = mob.get_mob_entity();
            mob_entity.set_no_ai(self.entity.mob_entity.is_no_ai());
            mob_entity.persistence_required.store(
                self.entity
                    .mob_entity
                    .persistence_required
                    .load(Ordering::Relaxed),
                Ordering::Relaxed,
            );
            if self.entity.is_baby() {
                mob.spawn_as_baby();
            }
        }

        world.spawn_entity(drowned);
        entity.remove();
    }
}
