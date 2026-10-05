use pumpkin_data::translation;
use pumpkin_util::PermissionLvl;
use pumpkin_util::permission::{Permission, PermissionDefault, PermissionRegistry};
use pumpkin_util::text::TextComponent;

use crate::command::argument_builder::{ArgumentBuilder, argument, command};
use crate::command::argument_types::function::FunctionArgumentType;
use crate::command::context::command_context::CommandContext;
use crate::command::errors::command_syntax_error::CommandSyntaxError;
use crate::command::errors::error_types::{CommandErrorType, LiteralCommandErrorType};
use crate::command::node::dispatcher::CommandDispatcher;
use crate::command::node::{CommandExecutor, CommandExecutorResult};
use crate::command::suggestion::provider::{SuggestionProvider, SuggestionProviderResult};
use crate::command::suggestion::suggestions::SuggestionsBuilder;
use crate::data::datapack::COMMAND_CHAIN_TOO_LONG;

const DESCRIPTION: &str = "Runs commands found in the corresponding function files.";
const PERMISSION: &str = "minecraft:command.function";

static ERROR_UNKNOWN_FUNCTION: CommandErrorType<1> = CommandErrorType::new(
    translation::java::ARGUMENTS_FUNCTION_UNKNOWN,
    translation::java::ARGUMENTS_FUNCTION_UNKNOWN,
);

static ERROR_COMMAND_CHAIN_TOO_LONG: LiteralCommandErrorType =
    LiteralCommandErrorType::new(COMMAND_CHAIN_TOO_LONG);

struct FunctionSuggestionProvider;

impl SuggestionProvider for FunctionSuggestionProvider {
    fn suggest(
        &self,
        context: &CommandContext,
        mut builder: SuggestionsBuilder,
    ) -> SuggestionProviderResult {
        let server = context.server();
        let function_names = server.datapack_manager.get_function_names();
        for name in function_names {
            builder = builder.suggest(name);
        }
        builder.build()
    }
}

struct FunctionExecutor;

impl CommandExecutor for FunctionExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let name_str = FunctionArgumentType::get(context, "name")?;
        let server = context.server();

        let executed_count =
            match server
                .datapack_manager
                .execute_function(server, &context.source, name_str)
            {
                Ok(executed_count) => executed_count,
                Err(err) => {
                    if err.contains(COMMAND_CHAIN_TOO_LONG) {
                        return Err(CommandSyntaxError::create_without_context(
                            &ERROR_COMMAND_CHAIN_TOO_LONG,
                            TextComponent::text(err),
                        ));
                    }
                    return Err(ERROR_UNKNOWN_FUNCTION
                        .create_without_context(TextComponent::text(name_str.to_string())));
                }
            };

        if name_str.starts_with('#') {
            context.source.send_feedback(
                TextComponent::translate_cross(
                    translation::java::COMMANDS_FUNCTION_SUCCESS_MULTIPLE,
                    translation::java::COMMANDS_FUNCTION_SUCCESS_MULTIPLE,
                    [
                        TextComponent::text(executed_count.to_string()),
                        TextComponent::text(name_str.to_string()),
                    ],
                ),
                true,
            );
        } else {
            context.source.send_feedback(
                TextComponent::translate_cross(
                    translation::java::COMMANDS_FUNCTION_SUCCESS_SINGLE,
                    translation::java::COMMANDS_FUNCTION_SUCCESS_SINGLE,
                    [
                        TextComponent::text(executed_count.to_string()),
                        TextComponent::text(name_str.to_string()),
                    ],
                ),
                true,
            );
        }

        Ok(executed_count as i32)
    }
}

pub fn register(dispatcher: &mut CommandDispatcher, registry: &PermissionRegistry) {
    registry.register_permission_or_panic(Permission::new(
        PERMISSION,
        DESCRIPTION,
        PermissionDefault::Op(PermissionLvl::Two),
    ));

    dispatcher.register(
        command("function", DESCRIPTION).requires(PERMISSION).then(
            argument("name", FunctionArgumentType)
                .suggests(FunctionSuggestionProvider)
                .executes(FunctionExecutor),
        ),
    );
}
