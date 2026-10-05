use std::any::Any;
use std::sync::atomic::Ordering;

use crate::entity::EntityBase;
use crate::entity::player::Player;
use crate::item::items::projectile_weapon::ProjectileWeaponItem;
use crate::item::{ItemBehaviour, ItemMetadata};
use pumpkin_data::data_component::DataComponent;
use pumpkin_data::data_component_impl::ChargedProjectilesImpl;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_util::GameMode;

pub struct CrossbowItem;

impl ItemMetadata for CrossbowItem {
    fn ids() -> Box<[u16]> {
        Box::new([Item::CROSSBOW.id])
    }
}

impl ItemBehaviour for CrossbowItem {
    fn normal_use(&self, _item: &Item, player: &Player) {
        let inventory = player.inventory();
        let stack = inventory.held_item();

        // Every crossbow carries a ChargedProjectiles component by default, so its mere
        // presence does not mean the crossbow is loaded. Vanilla checks the list is also
        // non-empty (CrossbowItem.java:68).
        if Self::is_charged(&stack) {
            Self::fire_projectiles(player);
            return;
        }

        let has_arrows = player.find_arrow().is_some();
        if !has_arrows && player.gamemode.load() != GameMode::Creative {
            return;
        }

        player
            .living_entity
            .set_active_hand(pumpkin_util::Hand::Right, stack, 72000);
    }

    fn on_stopped_using(&self, _stack: &ItemStack, player: &Player) {
        let use_ticks = player.living_entity.item_use_time.load(Ordering::Relaxed);
        let use_ticks = 72000 - use_ticks;

        let mut stack = player.inventory().held_item();
        let charge_time =
            crate::enchantment::EnchantmentHelper::modify_crossbow_charge_time(&stack, 25);

        if use_ticks >= charge_time {
            let arrow_slot = player.find_arrow();
            let gamemode = player.gamemode.load();
            let is_creative = gamemode == GameMode::Creative;

            if arrow_slot.is_some() || is_creative {
                let projectile = arrow_slot.map_or_else(
                    || ItemStack::new(1, &Item::ARROW),
                    |slot| {
                        let inventory = player.inventory();
                        inventory.get_slot(slot).copy_with_count(1)
                    },
                );

                let drawn = ProjectileWeaponItem::draw(&stack, &projectile, is_creative);
                if !drawn.is_empty() {
                    Self::store_charged_projectiles(&mut stack, &drawn);
                    player.inventory().set_held_item(stack);

                    if let Some(slot) = arrow_slot
                        && !is_creative
                    {
                        player.consume_arrow(slot);
                    }

                    player.world().play_sound(
                        Sound::ItemCrossbowLoadingEnd,
                        SoundCategory::Players,
                        &player.position(),
                    );
                }
            }
        }
        player.living_entity.clear_active_hand();
    }

    fn get_use_duration(&self) -> i32 {
        72000
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl CrossbowItem {
    pub const ARROW_POWER: f32 = 3.15;

    fn is_charged(stack: &ItemStack) -> bool {
        stack
            .get_data_component::<ChargedProjectilesImpl>()
            .is_some_and(|charged| {
                charged.projectiles.iter().any(|projectile| {
                    ItemStack::read_item_stack(projectile).is_some_and(|item| !item.is_empty())
                })
            })
    }

    fn store_charged_projectiles(stack: &mut ItemStack, drawn: &[ItemStack]) {
        let mut projectiles = Vec::with_capacity(drawn.len());
        for item in drawn {
            if item.is_empty() {
                continue;
            }
            let mut arrow_nbt = pumpkin_nbt::compound::NbtCompound::new();
            item.write_item_stack(&mut arrow_nbt);
            projectiles.push(arrow_nbt);
        }
        if projectiles.is_empty() {
            return;
        }
        stack.set_data_component(ChargedProjectilesImpl { projectiles });
    }

    fn fire_projectiles(player: &Player) {
        let mut held = player.inventory().held_item();
        let charged_opt = held.get_data_component::<ChargedProjectilesImpl>().cloned();

        if let Some(charged) = charged_opt {
            let mut projectiles = Vec::new();
            for projectile_nbt in charged.projectiles {
                if let Some(projectile) = ItemStack::read_item_stack(&projectile_nbt) {
                    projectiles.push(projectile);
                }
            }

            if !projectiles.is_empty() {
                let world = player.world();
                world.play_sound(
                    Sound::ItemCrossbowShoot,
                    SoundCategory::Players,
                    &player.position(),
                );

                let is_creative = player.gamemode.load() == GameMode::Creative;
                ProjectileWeaponItem::shoot_projectiles(
                    &world,
                    player.get_entity(),
                    &held,
                    &projectiles,
                    Self::ARROW_POWER,
                    1.0,
                    false,
                    is_creative,
                );

                held.patch
                    .retain(|(id, _)| *id != DataComponent::ChargedProjectiles);
                player.inventory().set_held_item(held);
                player.damage_held_item(1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CrossbowItem;
    use crate::item::items::projectile_weapon::ProjectileWeaponItem;
    use pumpkin_data::data_component::DataComponent;
    use pumpkin_data::item::Item;
    use pumpkin_data::item_stack::ItemStack;
    use pumpkin_protocol::codec::item_stack_seralizer::ItemStackSerializer;
    use pumpkin_protocol::ser::NetworkReadExt;
    use pumpkin_util::version::JavaMinecraftVersion;

    fn load_crossbow(is_creative: bool) -> ItemStack {
        let mut crossbow = ItemStack::new(1, &Item::CROSSBOW);
        let arrow = ItemStack::new(1, &Item::ARROW);
        let drawn = ProjectileWeaponItem::draw(&crossbow, &arrow, is_creative);
        CrossbowItem::store_charged_projectiles(&mut crossbow, &drawn);
        crossbow
    }

    fn read_varint(read: &mut &[u8]) -> Result<i32, String> {
        read.get_var_int()
            .map(|value| value.0)
            .map_err(|err| format!("{err:?}"))
    }

    fn skip_projectile_component(read: &mut &[u8], component_id: i32) -> Result<(), String> {
        if component_id == i32::from(DataComponent::IntangibleProjectile.to_id()) {
            let _ = read
                .get_nbt_with_version(&JavaMinecraftVersion::V_26_3)
                .map_err(|err| format!("{err:?}"))?;
            return Ok(());
        }
        Err(format!("unexpected projectile component {component_id}"))
    }

    fn assert_client_can_decode_loaded_crossbow(stack: &ItemStack) -> Result<(), String> {
        if !CrossbowItem::is_charged(stack) {
            return Err("loaded crossbow is not charged".into());
        }
        let charged = stack
            .get_data_component::<pumpkin_data::data_component_impl::ChargedProjectilesImpl>()
            .ok_or("missing charged projectiles")?;
        let stored_arrow = charged
            .projectiles
            .iter()
            .find_map(ItemStack::read_item_stack)
            .ok_or("charged projectile does not read back as an item")?;
        if stored_arrow.item.id != Item::ARROW.id || stored_arrow.is_empty() {
            return Err("charged projectile is not an arrow".into());
        }

        let mut bytes = Vec::new();
        ItemStackSerializer::from(stack.clone())
            .write(&mut bytes)
            .map_err(|err| format!("{err:?}"))?;
        let mut read = bytes.as_slice();

        let count = read_varint(&mut read)?;
        if count != 1 {
            return Err(format!("crossbow count {count}"));
        }
        let item_id = read_varint(&mut read)?;
        if item_id != i32::from(Item::CROSSBOW.id) {
            return Err(format!("item id {item_id}"));
        }
        let to_add = read_varint(&mut read)?;
        let to_remove = read_varint(&mut read)?;
        if to_add != 1 || to_remove != 0 {
            return Err(format!("component patch add={to_add} remove={to_remove}"));
        }
        let component_id = read_varint(&mut read)?;
        if component_id != i32::from(DataComponent::ChargedProjectiles.to_id()) {
            return Err(format!("component id {component_id}"));
        }

        let projectile_count = read_varint(&mut read)?;
        if projectile_count < 1 {
            return Err("charged crossbow has no projectile slot".into());
        }
        for _ in 0..projectile_count {
            let slot_count = read_varint(&mut read)?;
            if slot_count == 0 {
                return Err("empty projectile slot".into());
            }
            let projectile_id = read_varint(&mut read)?;
            if projectile_id != i32::from(Item::ARROW.id) {
                return Err(format!("projectile id {projectile_id}"));
            }
            let components_to_add = read_varint(&mut read)?;
            let components_to_remove = read_varint(&mut read)?;
            if components_to_add < 0 || components_to_remove < 0 {
                return Err("negative projectile component count".into());
            }
            for _ in 0..components_to_add {
                let component_id = read_varint(&mut read)?;
                skip_projectile_component(&mut read, component_id)?;
            }
            for _ in 0..components_to_remove {
                let _ = read_varint(&mut read)?;
            }
        }
        if !read.is_empty() {
            return Err(format!("{} trailing bytes", read.len()));
        }
        Ok(())
    }

    #[test]
    fn load_path_does_not_build_an_undecodable_charged_crossbow() {
        let survival = assert_client_can_decode_loaded_crossbow(&load_crossbow(false));
        assert_eq!(survival, Ok(()));
        let creative = assert_client_can_decode_loaded_crossbow(&load_crossbow(true));
        assert_eq!(creative, Ok(()));
    }
}
