use pumpkin_data::Block;
use pumpkin_data::BlockState;
use pumpkin_data::BlockStateId;
use pumpkin_data::damage::DamageType;
use pumpkin_data::entity::EntityType;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::tag::{self, Taggable};
use pumpkin_protocol::bedrock::client::CUpdateBlock;
use pumpkin_protocol::java::client::play::CBlockUpdate;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_world::world::BlockFlags;
use std::sync::atomic::AtomicU32;
use std::sync::{Arc, atomic::Ordering};

use crate::{
    block::blocks::falling::FallingBlock,
    entity::{Entity, EntityBase, living::LivingEntity},
    server::Server,
    world::World,
};

const LANDING_Y_EPSILON: f64 = 1.0e-4;
const OUT_OF_WORLD_TICKS: u32 = 100;
const MAX_FALL_TICKS: u32 = 600;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallingLand {
    Place,
    Drop,
    Discard,
}

pub struct FallingEntity {
    entity: Entity,
    block_state_id: BlockStateId,
    fall_ticks: AtomicU32,
}

impl FallingEntity {
    pub const fn new(entity: Entity, block_state_id: BlockStateId) -> Self {
        Self {
            entity,
            block_state_id,
            fall_ticks: AtomicU32::new(0),
        }
    }

    /// Replaced the current Block and Spawns a new Falling one (synchronous)
    pub fn replace_spawn(world: &Arc<World>, position: BlockPos, block_state: BlockStateId) {
        // Replace the original block, TODO: use fluid state
        world.set_block_state(
            &position,
            Block::AIR.default_state.id,
            BlockFlags::NOTIFY_ALL,
        );

        let position = position.0.to_f64().add_raw(0.5, 0.0, 0.5);
        let entity = Entity::new(world.clone(), position, &EntityType::FALLING_BLOCK);
        entity
            .data
            .store(i32::from(block_state.as_u16()), Ordering::Relaxed);
        let entity = Arc::new(Self::new(entity, block_state));
        world.spawn_entity_non_save(entity);
    }

    #[must_use]
    pub fn landing_block_y(y: f64) -> i32 {
        let floored = y.floor();
        let into_block = y - floored;
        if into_block > 1.0 - LANDING_Y_EPSILON {
            floored as i32 + 1
        } else {
            floored as i32
        }
    }

    #[must_use]
    pub fn landing_block_pos(pos: Vector3<f64>) -> BlockPos {
        BlockPos::new(
            pos.x.floor() as i32,
            Self::landing_block_y(pos.y),
            pos.z.floor() as i32,
        )
    }

    #[must_use]
    pub fn cell_accepts_falling_block(state: &BlockState) -> bool {
        let block = Block::from_state_id(state.id);
        FallingBlock::can_be_replaced_by_falling(state, block)
    }

    #[must_use]
    pub fn falling_land(
        landing: &BlockState,
        below: &BlockState,
        inside_world: bool,
    ) -> FallingLand {
        if !inside_world {
            return FallingLand::Discard;
        }
        if Self::cell_accepts_falling_block(landing) && !Self::cell_accepts_falling_block(below) {
            FallingLand::Place
        } else {
            FallingLand::Drop
        }
    }

    fn state_to_place(&self, world: &World, pos: &BlockPos) -> BlockStateId {
        let mut state_id = self.block_state_id;
        let block = Block::from_state_id(state_id);
        if block.has_tag(&tag::Block::MINECRAFT_CONCRETE_POWDERS)
            && FallingBlock::should_solidify(world, pos)
            && let Some(name) = block.name.strip_suffix("_powder")
            && let Some(concrete) = Block::from_name(name)
        {
            state_id = concrete.default_state.id;
        }
        state_id
    }

    fn drop_item(&self, world: &Arc<World>, pos: &BlockPos) {
        if !world.level_info.load().game_rules.entity_drops {
            return;
        }
        let block = Block::from_state_id(self.block_state_id);
        if block.item_id == 0 {
            return;
        }
        let Some(item) = Item::from_id(block.item_id) else {
            return;
        };
        world.drop_stack(pos, ItemStack::new(1, item));
    }

    fn complete_fall(&self, world: &Arc<World>, landing_pos: BlockPos) {
        let landing = world.get_block_state(&landing_pos);
        let below = world.get_block_state(&landing_pos.down());
        let entity = &self.entity;
        match Self::falling_land(landing, below, true) {
            FallingLand::Discard => {}
            FallingLand::Drop => self.drop_item(world, &landing_pos),
            FallingLand::Place => {
                let state_id = self.state_to_place(world, &landing_pos);
                world.set_block_state(&landing_pos, state_id, BlockFlags::NOTIFY_ALL);
                let placed = world.get_block_state_id(&landing_pos);
                if placed == state_id {
                    // block updates to watchers before the despawn, else a invisible block gap until the tick flush.
                    world.send_to_tracking_players_editioned(
                        entity,
                        &CBlockUpdate::new(landing_pos, i32::from(placed.as_u16()).into()),
                        &CUpdateBlock::new(landing_pos, BlockState::to_be_network_id(placed)),
                    );
                } else {
                    self.drop_item(world, &landing_pos);
                }
            }
        }
        entity.remove();
    }
}

impl EntityBase for FallingEntity {
    fn tick(&self, caller: &dyn EntityBase, _server: &Server) {
        let entity = &self.entity;
        let mut velo = entity.velocity.load();
        velo.y -= self.get_gravity();

        entity.velocity.store(velo);

        entity.move_entity(caller, velo);
        entity.tick_block_collisions(caller);

        let time = self
            .fall_ticks
            .fetch_add(1, Ordering::Relaxed)
            .saturating_add(1);
        let world = entity.world.load();
        let pos = entity.pos.load();
        if pos.y < f64::from(world.min_y) - 64.0 {
            entity.remove();
            return;
        }

        let on_ground = entity.on_ground.load(Ordering::Relaxed);
        if on_ground {
            entity.velocity.store(velo.multiply(0.7, -0.5, 0.7));
        }

        let landing_pos = Self::landing_block_pos(pos);
        let inside = world.is_in_build_limit(landing_pos);
        let finished = on_ground || time > MAX_FALL_TICKS || (time > OUT_OF_WORLD_TICKS && !inside);
        if !finished {
            entity.velocity.store(velo.multiply(0.98, 0.98, 0.98));
            return;
        }
        if !inside {
            entity.remove();
            return;
        }
        self.complete_fall(&world, landing_pos);
    }

    fn init_data_tracker(&self) {
        self.entity.set_synced_data(
            pumpkin_data::tracked_data::falling_block::START_POS,
            self.entity.block_pos.load(),
        );
    }

    fn get_entity(&self) -> &Entity {
        &self.entity
    }

    fn get_living_entity(&self) -> Option<&LivingEntity> {
        None
    }
    fn damage(&self, _caller: &dyn EntityBase, _amount: f32, _damage_type: DamageType) -> bool {
        false
    }

    fn get_gravity(&self) -> f64 {
        0.04
    }

    // TODO: Bedrock spawn metadata lacks the block variant (renders grey while falling)
    fn bedrock_y_offset(&self) -> f64 {
        0.49
    }

    fn cast_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{FallingEntity, FallingLand};
    use pumpkin_data::Block;
    use pumpkin_data::block_properties::SnowLikeProperties;

    fn snow(layers: u8) -> &'static pumpkin_data::BlockState {
        SnowLikeProperties { layers }
            .to_state_id(&Block::SNOW)
            .to_state()
    }

    fn land(landing: &pumpkin_data::BlockState, below: &pumpkin_data::BlockState) -> FallingLand {
        FallingEntity::falling_land(landing, below, true)
    }

    #[test]
    fn replaceable_cells_accept_a_falling_block() {
        let stone = Block::STONE.default_state;
        assert_eq!(land(Block::AIR.default_state, stone), FallingLand::Place);
        assert_eq!(
            land(Block::SHORT_GRASS.default_state, stone),
            FallingLand::Place
        );
        assert_eq!(land(Block::FIRE.default_state, stone), FallingLand::Place);
        assert_eq!(land(snow(1), stone), FallingLand::Place);
    }

    #[test]
    fn partial_blocks_drop_and_stay() {
        let stone = Block::STONE.default_state;
        assert_eq!(land(Block::TORCH.default_state, stone), FallingLand::Drop);
        assert_eq!(
            land(Block::OAK_SAPLING.default_state, stone),
            FallingLand::Drop
        );
        assert_eq!(
            land(Block::OAK_BUTTON.default_state, stone),
            FallingLand::Drop
        );
        assert_eq!(land(snow(2), stone), FallingLand::Drop);
        assert_eq!(land(snow(8), stone), FallingLand::Drop);
        assert_eq!(land(stone, Block::AIR.default_state), FallingLand::Drop);
    }

    #[test]
    fn unstable_and_out_of_world_falls_do_not_place() {
        assert_eq!(
            land(Block::AIR.default_state, Block::AIR.default_state),
            FallingLand::Drop
        );
        assert_eq!(
            FallingEntity::falling_land(
                Block::AIR.default_state,
                Block::STONE.default_state,
                false
            ),
            FallingLand::Discard
        );
        assert_eq!(
            FallingEntity::falling_land(
                Block::TORCH.default_state,
                Block::STONE.default_state,
                false
            ),
            FallingLand::Discard
        );
    }

    #[test]
    fn landing_y_just_under_a_boundary_uses_the_block_above() {
        assert_eq!(FallingEntity::landing_block_y(64.0), 64);
        assert_eq!(FallingEntity::landing_block_y(64.5), 64);
        assert_eq!(FallingEntity::landing_block_y(64.0 - 1.0e-10), 64);
        assert_eq!(FallingEntity::landing_block_y(-1.0e-16), 0);
        assert_eq!(FallingEntity::landing_block_y(63.9), 63);
    }
}
