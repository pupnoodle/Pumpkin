use pumpkin_data::fluid::Fluid;
use pumpkin_data::tag::{self, Taggable};
use pumpkin_data::{Block, BlockState, BlockStateId};
use pumpkin_macros::pumpkin_block;
use pumpkin_util::math::position::BlockPos;
use pumpkin_world::world::{BlockAccessor, BlockFlags};

use crate::block::{GetStateForNeighborUpdateArgs, blocks::plant::PlantBlockBase};

use crate::block::{BlockBehaviour, CanPlaceAtArgs, OnEntityCollisionArgs};

#[pumpkin_block("minecraft:lily_pad")]
pub struct LilyPadBlock;

impl BlockBehaviour for LilyPadBlock {
    fn on_entity_collision(&self, args: OnEntityCollisionArgs<'_>) {
        {
            // Proberbly not the best solution, but works
            if args
                .entity
                .get_entity()
                .entity_type
                .resource_name
                .ends_with("_boat")
            {
                args.world
                    .break_block(args.position, None, BlockFlags::NOTIFY_ALL);
            }
        }
    }

    fn can_place_at(&self, args: CanPlaceAtArgs<'_>) -> bool {
        <Self as PlantBlockBase>::can_place_at(self, args.block_accessor, args.position)
    }

    fn get_state_for_neighbor_update(
        &self,
        args: GetStateForNeighborUpdateArgs<'_>,
    ) -> BlockStateId {
        <Self as PlantBlockBase>::get_state_for_neighbor_update(
            self,
            args.world,
            args.position,
            args.state_id,
        )
    }
}

impl PlantBlockBase for LilyPadBlock {
    fn can_plant_on_top(&self, block_accessor: &dyn BlockAccessor, pos: &BlockPos) -> bool {
        let (block, state) = block_accessor.get_block_and_state(pos);
        let above = block_accessor.get_block(&pos.up());
        above.is_air() && supports_lily_pad(block, state)
    }
}

fn supports_lily_pad(block: &Block, state: &BlockState) -> bool {
    state.is_waterlogged()
        || block.has_tag(&tag::Block::MINECRAFT_SUPPORTS_LILY_PAD)
        || Fluid::from_state_id(state.id)
            .is_some_and(|fluid| fluid.has_tag(&tag::Fluid::MINECRAFT_SUPPORTS_LILY_PAD))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use pumpkin_data::block_properties::{
        SlabType, WaterLikeProperties, WhiteWoolSlabLikeProperties,
    };
    use pumpkin_data::{Block, BlockState, BlockStateId};
    use pumpkin_util::math::position::BlockPos;
    use pumpkin_world::world::BlockAccessor;

    use super::LilyPadBlock;
    use super::PlantBlockBase;

    struct Grid(HashMap<BlockPos, BlockStateId>);

    impl Grid {
        fn new() -> Self {
            Self(HashMap::new())
        }

        fn set(&mut self, pos: BlockPos, state: BlockStateId) {
            self.0.insert(pos, state);
        }
    }

    impl BlockAccessor for Grid {
        fn get_block(&self, position: &BlockPos) -> &'static Block {
            Block::from_state_id(self.get_block_state_id(position))
        }

        fn get_block_state(&self, position: &BlockPos) -> &'static BlockState {
            BlockState::from_id(self.get_block_state_id(position))
        }

        fn get_block_state_id(&self, position: &BlockPos) -> BlockStateId {
            self.0
                .get(position)
                .copied()
                .unwrap_or(Block::AIR.default_state.id)
        }

        fn get_block_and_state(
            &self,
            position: &BlockPos,
        ) -> (&'static Block, &'static BlockState) {
            let state = self.get_block_state(position);
            (Block::from_state_id(state.id), state)
        }
    }

    fn can_plant(grid: &Grid, support: BlockStateId) -> bool {
        let mut placed = Grid::new();
        placed.set(BlockPos::new(0, 0, 0), support);
        if grid.0.contains_key(&BlockPos::new(0, 1, 0)) {
            placed.set(
                BlockPos::new(0, 1, 0),
                grid.get_block_state_id(&BlockPos::new(0, 1, 0)),
            );
        }
        LilyPadBlock.can_plant_on_top(&placed, &BlockPos::new(0, 0, 0))
    }

    fn waterlogged_top_slab() -> BlockStateId {
        let block = &Block::OAK_SLAB;
        let mut props = WhiteWoolSlabLikeProperties::default(block);
        props.r#type = SlabType::Top;
        props.waterlogged = true;
        props.to_state_id(block)
    }

    #[test]
    fn lily_pad_accepts_water_waterlogged_blocks_ice_and_frosted_ice() {
        let empty = Grid::new();
        assert!(can_plant(&empty, Block::WATER.default_state.id));
        let slab = waterlogged_top_slab();
        assert!(BlockState::from_id(slab).is_waterlogged());
        assert!(can_plant(&empty, slab));
        assert!(can_plant(&empty, Block::ICE.default_state.id));
        assert!(can_plant(&empty, Block::FROSTED_ICE.default_state.id));
    }

    #[test]
    fn lily_pad_rejects_dry_slabs_flowing_water_and_occupied_space() {
        let empty = Grid::new();
        let dry = {
            let block = &Block::OAK_SLAB;
            let mut props = WhiteWoolSlabLikeProperties::default(block);
            props.r#type = SlabType::Top;
            props.to_state_id(block)
        };
        assert!(!BlockState::from_id(dry).is_waterlogged());
        assert!(!can_plant(&empty, dry));
        assert!(!can_plant(&empty, Block::AIR.default_state.id));

        let flowing = WaterLikeProperties { level: 1 }.to_state_id(&Block::WATER);
        assert!(!can_plant(&empty, flowing));

        let mut blocked = Grid::new();
        blocked.set(BlockPos::new(0, 1, 0), Block::STONE.default_state.id);
        assert!(!can_plant(&blocked, Block::WATER.default_state.id));
        assert!(!can_plant(&blocked, waterlogged_top_slab()));
    }
}
