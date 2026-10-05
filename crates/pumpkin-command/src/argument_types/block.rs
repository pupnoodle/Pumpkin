use crate::{
    argument_types::argument_type::{ArgumentType, JavaClientArgumentType},
    context::command_context::CommandContext,
    errors::command_syntax_error::CommandSyntaxError,
    errors::error_types::CommandErrorType,
    string_reader::StringReader,
    suggestion::suggestions::{Suggestions, SuggestionsBuilder},
};
use pumpkin_data::{Block, BlockStateId, translation};
use pumpkin_util::text::TextComponent;

pub const INVALID_BLOCK_ERROR_TYPE: CommandErrorType<1> = CommandErrorType::new(
    translation::java::ARGUMENT_BLOCK_ID_INVALID,
    translation::java::ARGUMENT_BLOCK_ID_INVALID,
);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct BlockStateArgument {
    pub block: &'static Block,
    pub state: BlockStateId,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct BlockArgumentType;

impl<S: crate::source::CommandSource> ArgumentType<S> for BlockArgumentType {
    type Item = BlockStateArgument;

    fn parse(&self, reader: &mut StringReader) -> Result<Self::Item, CommandSyntaxError> {
        let start = reader.cursor();
        while let Some(c) = reader.peek() {
            if c.is_alphanumeric() || c == '_' || c == ':' || c == '/' || c == '.' || c == '-' {
                reader.skip();
            } else {
                break;
            }
        }
        let block_name = &reader.string()[start..reader.cursor()];
        let normalized = if block_name.contains(':') {
            block_name.to_string()
        } else {
            format!("minecraft:{block_name}")
        };

        let block = Block::from_name(&normalized)
            .ok_or_else(|| INVALID_BLOCK_ERROR_TYPE.create(reader, TextComponent::text(normalized.clone())))?;

        let state = if reader.peek() == Some('[') {
            reader.skip();
            let mut properties: Vec<(String, String)> = Vec::new();
            loop {
                let key_start = reader.cursor();
                while let Some(c) = reader.peek() {
                    if c == '=' || c == ']' || c == ',' {
                        break;
                    }
                    reader.skip();
                }
                let key = reader.string()[key_start..reader.cursor()].to_string();
                if key.is_empty() || reader.peek() != Some('=') {
                    return Err(INVALID_BLOCK_ERROR_TYPE.create(
                        reader,
                        TextComponent::text(normalized.clone()),
                    ));
                }
                reader.skip();
                let value_start = reader.cursor();
                while let Some(c) = reader.peek() {
                    if c == ']' || c == ',' {
                        break;
                    }
                    reader.skip();
                }
                let value = reader.string()[value_start..reader.cursor()].to_string();
                if value.is_empty() {
                    return Err(INVALID_BLOCK_ERROR_TYPE.create(
                        reader,
                        TextComponent::text(normalized.clone()),
                    ));
                }
                properties.push((key, value));
                match reader.peek() {
                    Some(']') => {
                        reader.skip();
                        break;
                    }
                    Some(',') => {
                        reader.skip();
                    }
                    _ => {
                        return Err(INVALID_BLOCK_ERROR_TYPE.create(
                            reader,
                            TextComponent::text(normalized.clone()),
                        ));
                    }
                }
            }

            let mut merged: Vec<(String, String)> = block
                .properties(block.default_state.id)
                .map(|props| {
                    props
                        .to_props()
                        .into_iter()
                        .map(|(name, value)| (name.to_string(), value.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            for (key, value) in &properties {
                if let Some(entry) = merged.iter_mut().find(|(name, _)| name == key) {
                    entry.1 = value.clone();
                } else {
                    return Err(INVALID_BLOCK_ERROR_TYPE.create(
                        reader,
                        TextComponent::text(normalized.clone()),
                    ));
                }
            }
            let merged_refs: Vec<(&str, &str)> = merged
                .iter()
                .map(|(name, value)| (name.as_str(), value.as_str()))
                .collect();
            block
                .state_from_properties(&merged_refs)
                .map(|state| state.id)
                .ok_or_else(|| INVALID_BLOCK_ERROR_TYPE.create(reader, TextComponent::text(normalized)))?
        } else {
            block.default_state.id
        };

        Ok(BlockStateArgument { block, state })
    }

    fn client_side_parser(&'_ self) -> JavaClientArgumentType {
        JavaClientArgumentType::BlockState
    }

    fn list_suggestions(
        &self,
        _context: &CommandContext<S>,
        builder: SuggestionsBuilder,
    ) -> Suggestions {
        builder.build()
    }
}

impl BlockArgumentType {
    pub fn get<S: crate::source::CommandSource>(
        context: &CommandContext<S>,
        name: &str,
    ) -> Result<BlockStateArgument, CommandSyntaxError> {
        context.get_argument::<BlockStateArgument>(name).copied()
    }
}
