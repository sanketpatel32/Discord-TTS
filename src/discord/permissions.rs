use poise::serenity_prelude as serenity;

use crate::config::Config;

pub async fn is_operator(
    ctx: &poise::Context<'_, crate::AppData, crate::Error>,
    config: &Config,
) -> bool {
    let author_id = ctx.author().id.get();

    // Check bot owners
    if config.bot_owner_user_ids.contains(&author_id) {
        return true;
    }

    // Check guild permissions and controller role
    if let Some(guild_id) = ctx.guild_id() {
        let gid = guild_id.get();
        if let Some(member) = ctx.author_member().await.as_deref() {
            if let Some(guild) = ctx.cache().guild(guild_id) {
                // If member is owner of guild
                if guild.owner_id == ctx.author().id {
                    return true;
                }
            }

            #[allow(deprecated)]
            if let Ok(perms) = member.permissions(ctx.cache()) {
                if perms.contains(serenity::Permissions::MANAGE_GUILD)
                    || perms.contains(serenity::Permissions::ADMINISTRATOR)
                {
                    return true;
                }
            }

            // Check controller role
            if let Some(&role_id) = config.controller_role_ids_by_guild.get(&gid) {
                if member.roles.iter().any(|r| r.get() == role_id) {
                    return true;
                }
            }
        }
    }

    false
}

pub fn get_user_voice_channel(
    ctx: &poise::Context<'_, crate::AppData, crate::Error>,
) -> Option<serenity::ChannelId> {
    let guild_id = ctx.guild_id()?;
    let user_id = ctx.author().id;
    let guild = ctx.cache().guild(guild_id)?;
    guild.voice_states.get(&user_id)?.channel_id
}
