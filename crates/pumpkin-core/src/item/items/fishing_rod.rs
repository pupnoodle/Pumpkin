use std::any::Any;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use crate::block::registry::BlockActionResult;
use crate::entity::Entity;
use crate::entity::player::Player;
use crate::entity::projectile::fishing_bobber::FishingBobberEntity;
use crate::item::{ItemBehaviour, ItemMetadata};
use crate::server::Server;
use pumpkin_data::data_component_impl::EquipmentSlot;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::{Block, BlockDirection};
use pumpkin_util::Hand;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;

pub struct FishingRodItem;

impl ItemMetadata for FishingRodItem {
    fn ids() -> Box<[u16]> {
        Box::new([Item::FISHING_ROD.id])
    }
}

impl FishingRodItem {
    fn cast_or_reel(player: &Player, yaw: f32, pitch: f32) -> i32 {
        let world = player.world();
        let bobber_id = player.fishing_bobber.load(Ordering::Relaxed);

        if bobber_id == -1 {
            world.play_sound(
                Sound::EntityFishingBobberThrow,
                SoundCategory::Neutral,
                &player.position(),
            );

            let bobber_entity = Entity::new(
                world.clone(),
                player.position(),
                &pumpkin_data::entity::EntityType::FISHING_BOBBER,
            );
            let bobber = FishingBobberEntity::new(bobber_entity, player);
            bobber.shoot_towards(yaw, pitch);

            let bobber_arc: Arc<FishingBobberEntity> = Arc::new(bobber);
            let spawned_id = bobber_arc.entity.entity_id;
            if world.spawn_entity(bobber_arc) {
                player.fishing_bobber.store(spawned_id, Ordering::Relaxed);
            }
            0
        } else {
            let damage = world.get_entity_by_id(bobber_id).map_or(0, |bobber_base| {
                let damage = bobber_base
                    .cast_any()
                    .downcast_ref::<FishingBobberEntity>()
                    .map_or(0, |bobber| bobber.reel_in(player));
                bobber_base.get_entity().remove();
                damage
            });
            player.fishing_bobber.store(-1, Ordering::Relaxed);
            world.play_sound(
                Sound::EntityFishingBobberRetrieve,
                SoundCategory::Neutral,
                &player.position(),
            );
            damage
        }
    }
}

impl ItemBehaviour for FishingRodItem {
    fn normal_use(&self, item: &Item, player: &Player) {
        let (yaw, pitch) = player.rotation();
        self.normal_use_with_hand(item, player, yaw, pitch, Hand::Right);
    }

    fn normal_use_with_hand(
        &self,
        _item: &Item,
        player: &Player,
        yaw: f32,
        pitch: f32,
        hand: Hand,
    ) {
        let damage = Self::cast_or_reel(player, yaw, pitch);
        if damage > 0 {
            let slot = match hand {
                Hand::Right => EquipmentSlot::MAIN_HAND,
                Hand::Left => EquipmentSlot::OFF_HAND,
            };
            player.damage_item_in_slot(&slot, damage);
        }
    }

    fn use_on_block(
        &self,
        item: &mut ItemStack,
        player: &Player,
        _location: BlockPos,
        _face: BlockDirection,
        _cursor_pos: Vector3<f32>,
        _block: &Block,
        _server: &Server,
    ) -> BlockActionResult {
        let (yaw, pitch) = player.rotation();
        let damage = Self::cast_or_reel(player, yaw, pitch);
        if damage > 0 && !player.is_creative() && !player.is_spectator() {
            let _ = item.damage_item(damage);
        }
        BlockActionResult::Success
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
