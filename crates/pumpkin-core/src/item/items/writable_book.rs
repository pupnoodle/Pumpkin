use std::any::Any;

use crate::entity::player::Player;
use crate::item::{ItemBehaviour, ItemMetadata};
use pumpkin_data::item::Item;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_util::Hand;

pub struct WritableBookItem;

const fn should_send_open_book(item: &Item) -> bool {
    item.id == Item::WRITTEN_BOOK.id
}

impl ItemMetadata for WritableBookItem {
    fn ids() -> Box<[u16]> {
        Box::new([Item::WRITABLE_BOOK.id, Item::WRITTEN_BOOK.id])
    }
}

impl ItemBehaviour for WritableBookItem {
    fn normal_use_with_hand(
        &self,
        item: &Item,
        player: &Player,
        _yaw: f32,
        _pitch: f32,
        hand: Hand,
    ) {
        // The client opens a book and quill by itself when the player uses it.
        // Sending the open-book packet for it makes the client open the book
        // again and it lands in the read-only view, where nothing can be
        // written. A signed book is not opened by the client, so the server has
        // to send the packet for that one.
        if should_send_open_book(item) {
            player.open_book(hand);
        }
        player.world().play_sound(
            Sound::ItemBookPageTurn,
            SoundCategory::Players,
            &player.position(),
        );
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use pumpkin_data::data_component_impl::WrittenBookContentImpl;
    use pumpkin_data::item::Item;
    use pumpkin_nbt::compound::NbtCompound;
    use pumpkin_nbt::tag::NbtTag;

    use super::should_send_open_book;

    fn filterable(raw: &str) -> NbtTag {
        let mut compound = NbtCompound::new();
        compound.put_string("raw", raw.to_string());
        NbtTag::Compound(compound)
    }

    #[test]
    fn writable_book_stays_on_the_editor_screen() {
        assert!(!should_send_open_book(&Item::WRITABLE_BOOK));
        assert!(should_send_open_book(&Item::WRITTEN_BOOK));
    }

    #[test]
    fn written_book_reader_keeps_raw_filterable_text() {
        let mut book = NbtCompound::new();
        book.put("title", filterable("A Partner"));
        book.put_string("author", "Dylan Collins".to_string());
        book.put(
            "pages",
            NbtTag::List(vec![
                filterable("I'm glad you got here"),
                NbtTag::String("plain page".into()),
            ]),
        );

        let content = WrittenBookContentImpl::read_data(&NbtTag::Compound(book)).unwrap();
        assert_eq!(content.title, "A Partner");
        assert_eq!(content.author, "Dylan Collins");
        assert_eq!(
            content
                .pages
                .iter()
                .map(|page| page.clone().get_text())
                .collect::<Vec<_>>(),
            ["I'm glad you got here", "plain page"]
        );
    }
}
