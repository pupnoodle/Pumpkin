use crate::block::registry::BlockActionResult;
use crate::entity::player::Player;
use crate::item::{ItemBehaviour, ItemMetadata};
use crate::server::Server;
use pumpkin_data::BlockDirection;
use pumpkin_data::block_properties::FarmlandLikeProperties;
use pumpkin_data::block_transformer::{DropStrategy, HOE};
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag::Taggable;
use pumpkin_data::{Block, BlockStateId, tag};
use pumpkin_util::GameMode;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_world::world::BlockFlags;

pub struct HoeItem;

impl ItemMetadata for HoeItem {
    fn ids() -> Box<[u16]> {
        tag::Item::MINECRAFT_HOES.1.into()
    }
}

impl ItemBehaviour for HoeItem {
    fn use_on_block(
        &self,
        item: &mut ItemStack,
        player: &Player,
        location: BlockPos,
        face: BlockDirection,
        _cursor_pos: Vector3<f32>,
        block: &Block,
        _server: &Server,
    ) -> BlockActionResult {
        let world = player.world();
        let get_block = |dx: i8, dy: i8, dz: i8| {
            let check_pos = BlockPos(location.0 + Vector3::new(dx as i32, dy as i32, dz as i32));
            world.get_block(&check_pos)
        };

        if let Some(result) =
            HOE.transform(block, world.get_block_state_id(&location), face, &get_block)
        {
            if let Some(sound) = result.entry.sound {
                world.play_sound(sound, SoundCategory::Blocks, &location.to_f64());
            }
            if let Some(particle) = result.entry.particle {
                world.sync_world_event(particle, location, 0);
            }

            world.set_block_state(&location, result.new_state_id, BlockFlags::NOTIFY_ALL);

            if let Some(loot_key) = result.entry.loot
                && let Some(loot_table) = world.get_loot_table(loot_key)
            {
                let seed = rand::random::<i64>();
                let drops = loot_table.generate_loot(seed);
                for drop_stack in drops {
                    if result.entry.drop_strategy == Some(DropStrategy::ClickedFace) {
                        world.drop_stack_from_face(&location, face, drop_stack);
                    } else {
                        world.drop_stack(&location, drop_stack);
                    }
                }
            }

            if hoe_use_damages(player.gamemode.load()) {
                let _ = item.damage_item(i32::from(result.entry.item_damage_per_use));
            }
            return BlockActionResult::Success;
        }

        let above = world.get_block(&location.up());
        if hoe_tills(block, above, face) {
            world.play_sound(
                Sound::ItemHoeTill,
                SoundCategory::Blocks,
                &location.to_f64(),
            );
            world.set_block_state(&location, tilled_farmland_state(), BlockFlags::NOTIFY_ALL);
            if hoe_use_damages(player.gamemode.load()) {
                let _ = item.damage_item(1);
            }
            return BlockActionResult::Success;
        }
        BlockActionResult::Pass
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

fn hoe_tills(block: &Block, above: &Block, face: BlockDirection) -> bool {
    face != BlockDirection::Down
        && block.has_tag(&tag::Block::MINECRAFT_TURNS_INTO_FARMLAND)
        && above.has_tag(&tag::Block::MINECRAFT_AIR)
}

fn tilled_farmland_state() -> BlockStateId {
    let mut props = FarmlandLikeProperties::default(&Block::FARMLAND);
    props.moisture = 0;
    props.to_state_id(&Block::FARMLAND)
}

fn hoe_use_damages(gamemode: GameMode) -> bool {
    gamemode != GameMode::Creative
}

#[cfg(test)]
mod tests {
    use super::*;
    use pumpkin_data::item::Item;

    #[test]
    fn hoe_tills_dirt_and_grass_with_air_above_and_damages_in_survival() {
        let farmland = tilled_farmland_state();
        assert!(hoe_tills(&Block::DIRT, &Block::AIR, BlockDirection::Up));
        assert!(hoe_tills(
            &Block::GRASS_BLOCK,
            &Block::AIR,
            BlockDirection::North
        ));
        assert_eq!(FarmlandLikeProperties::from_state_id(farmland).moisture, 0);
        assert_eq!(Block::from_state_id(farmland), &Block::FARMLAND);

        assert!(!hoe_tills(&Block::DIRT, &Block::AIR, BlockDirection::Down));
        assert!(!hoe_tills(&Block::DIRT, &Block::STONE, BlockDirection::Up));
        assert!(!hoe_tills(&Block::STONE, &Block::AIR, BlockDirection::Up));

        assert!(hoe_use_damages(GameMode::Survival));
        assert!(!hoe_use_damages(GameMode::Creative));

        let mut hoe = ItemStack::new(1, &Item::WOODEN_HOE);
        let before = hoe.get_damage();
        let _ = hoe.damage_item(1);
        assert_eq!(hoe.get_damage(), before + 1);
    }
}
