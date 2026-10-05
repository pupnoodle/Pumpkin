use pumpkin_data::BlockStateId;
use pumpkin_data::block_properties::HorizontalFacing;
use pumpkin_data::fluid::Fluid;
use pumpkin_data::tag::Taggable;
use pumpkin_data::{Block, BlockState, block_properties::CactusLikeProperties, tag};
use pumpkin_macros::pumpkin_block;
use pumpkin_util::math::position::BlockPos;
use pumpkin_world::tick::TickPriority;
use pumpkin_world::world::{BlockAccessor, BlockFlags};

use crate::block::{
    BlockBehaviour, CanPlaceAtArgs, GetStateForNeighborUpdateArgs, OnScheduledTickArgs,
    RandomTickArgs,
};

#[pumpkin_block("minecraft:sugar_cane")]
pub struct SugarCaneBlock;

impl BlockBehaviour for SugarCaneBlock {
    fn on_scheduled_tick(&self, args: OnScheduledTickArgs<'_>) {
        if !can_place_at(args.world.as_ref(), args.position) {
            args.world
                .break_block(args.position, None, BlockFlags::NOTIFY_ALL);
        }
    }

    fn random_tick(&self, args: RandomTickArgs<'_>) {
        if args.world.get_block_state(&args.position.up()).is_air()
            && !(args.world.get_block(&args.position.down()) == &Block::SUGAR_CANE
                && args.world.get_block(&args.position.down().down()) == &Block::SUGAR_CANE)
        {
            let state_id = args.world.get_block_state(args.position).id;
            let age = CactusLikeProperties::from_state_id(state_id).age;
            if age == 15 {
                args.world
                    .set_block_state(&args.position.up(), state_id, BlockFlags::NOTIFY_ALL);
                let props = CactusLikeProperties { age: 0 };
                args.world.set_block_state(
                    args.position,
                    props.to_state_id(args.block),
                    BlockFlags::NOTIFY_LISTENERS,
                );
            } else {
                let props = CactusLikeProperties { age: age + 1 };
                args.world.set_block_state(
                    args.position,
                    props.to_state_id(args.block),
                    BlockFlags::NOTIFY_LISTENERS,
                );
            }
        }
    }

    fn get_state_for_neighbor_update(
        &self,
        args: GetStateForNeighborUpdateArgs<'_>,
    ) -> BlockStateId {
        if !can_place_at(args.world, args.position) {
            args.world
                .schedule_block_tick(args.block, *args.position, 1, TickPriority::Normal);
        }
        args.state_id
    }

    fn can_place_at(&self, args: CanPlaceAtArgs<'_>) -> bool {
        can_place_at(args.block_accessor, args.position)
    }
}

fn can_place_at(block_accessor: &dyn BlockAccessor, block_pos: &BlockPos) -> bool {
    let block_below = block_accessor.get_block(&block_pos.down());

    if block_below == &Block::SUGAR_CANE {
        return true;
    }

    if block_below.has_tag(&tag::Block::MINECRAFT_SUPPORTS_SUGAR_CANE) {
        let below = block_pos.down();
        for direction in HorizontalFacing::all() {
            let neighbor = below.offset(direction.to_offset());
            let (block, state) = block_accessor.get_block_and_state(&neighbor);
            if supports_sugar_cane_adjacently(block, state) {
                return true;
            }
        }
    }

    false
}

fn supports_sugar_cane_adjacently(block: &Block, state: &BlockState) -> bool {
    block.has_tag(&tag::Block::MINECRAFT_SUPPORTS_SUGAR_CANE_ADJACENTLY)
        || state.is_waterlogged()
        || Fluid::from_state_id(state.id).is_some_and(|fluid| {
            fluid.has_tag(&tag::Fluid::MINECRAFT_SUPPORTS_SUGAR_CANE_ADJACENTLY)
        })
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

    use super::can_place_at;

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

    fn flowing_water() -> BlockStateId {
        WaterLikeProperties { level: 1 }.to_state_id(&Block::WATER)
    }

    fn waterlogged_top_slab() -> BlockStateId {
        let block = &Block::OAK_SLAB;
        let mut props = WhiteWoolSlabLikeProperties::default(block);
        props.r#type = SlabType::Top;
        props.waterlogged = true;
        props.to_state_id(block)
    }

    fn cane_on_sand_beside(neighbor: BlockStateId) -> bool {
        let mut grid = Grid::new();
        let cane = BlockPos::new(0, 1, 0);
        grid.set(BlockPos::new(0, 0, 0), Block::SAND.default_state.id);
        grid.set(BlockPos::new(1, 0, 0), neighbor);
        can_place_at(&grid, &cane)
    }

    #[test]
    fn sand_beside_water_flowing_water_waterlogged_or_frosted_ice_accepts_cane() {
        assert!(cane_on_sand_beside(Block::WATER.default_state.id));
        assert!(cane_on_sand_beside(flowing_water()));
        let slab = waterlogged_top_slab();
        assert!(BlockState::from_id(slab).is_waterlogged());
        assert!(cane_on_sand_beside(slab));
        assert!(cane_on_sand_beside(Block::FROSTED_ICE.default_state.id));
    }

    #[test]
    fn dry_neighbor_or_unsupported_block_rejects_cane() {
        assert!(!cane_on_sand_beside(Block::STONE.default_state.id));
        assert!(!cane_on_sand_beside(Block::AIR.default_state.id));

        let mut grid = Grid::new();
        grid.set(BlockPos::new(0, 0, 0), Block::STONE.default_state.id);
        grid.set(BlockPos::new(1, 0, 0), Block::WATER.default_state.id);
        assert!(!can_place_at(&grid, &BlockPos::new(0, 1, 0)));
    }

    #[test]
    fn cane_stacked_on_cane_does_not_need_water() {
        let mut grid = Grid::new();
        grid.set(BlockPos::new(0, 0, 0), Block::SUGAR_CANE.default_state.id);
        assert!(can_place_at(&grid, &BlockPos::new(0, 1, 0)));
    }
}
