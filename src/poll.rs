use anyhow::Context as _;
use once_cell::sync::Lazy;
use serenity::{
    builder::{CreateActionRow, CreateButton},
    model::{
        id::{GuildId, InteractionId, UserId},
        interactions::{
            ApplicationCommand, ApplicationCommandInteractionData, ApplicationCommandOptionType,
            ButtonStyle, Interaction, InteractionResponseType, MessageComponent,
        },
    },
    prelude::*,
};
use std::{
    collections::{BTreeMap, HashMap},
    fmt::Display,
    time::Duration,
};
use tokio::time::Instant;

pub const COMMAND: &str = "poll";

pub async fn create(guild_id: GuildId, ctx: &Context) -> anyhow::Result<ApplicationCommand> {
    let res = guild_id
        .create_application_command(&ctx, |command| {
            command
                .name(COMMAND)
                .description("a simple poll command")
                .create_option(|option| {
                    option
                        .name("options")
                        .kind(ApplicationCommandOptionType::String)
                        .description("comma separated list of options")
                        .required(true)
                })
        })
        .await?;
    Ok(res)
}

pub async fn start(
    ctx: &Context,
    interaction: &Interaction,
    command: &ApplicationCommandInteractionData,
) -> anyhow::Result<()> {
    // collect and validate poll options
    let mut options = command
        .options
        .iter()
        .find(|o| o.name == "options")
        .context("missing options")?
        .value
        .as_ref()
        .context("missing options value")?
        .as_str()
        .context("invalid options value")?
        .split(",")
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    if options.is_empty() {
        anyhow::bail!("no options");
    }
    options.sort();
    options.dedup();

    // poll data is stored in a static to be accessed for voting and cleanup
    let poll_data = PollData {
        start_time: Instant::now(),
        options: options.iter().copied().map(String::from).collect(),
        votes: HashMap::new(),
    };

    // respond with poll
    interaction
        .create_interaction_response(&ctx.http, |response| {
            response
                .kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|response_data| {
                    response_data
                        .content(create_content(&poll_data))
                        .components(|components| {
                            // create voting buttons
                            let mut row = CreateActionRow::default();
                            for option in options.iter().copied() {
                                let mut button = CreateButton::default();
                                button
                                    .custom_id(option)
                                    .label(option)
                                    .style(ButtonStyle::Primary);
                                row.add_button(button);
                            }
                            components.add_action_row(row)
                        })
                })
        })
        .await?;

    // on success, store poll data
    let mut lock = POLLS.write().await;
    lock.insert(interaction.id, poll_data);
    Ok(())
}

pub async fn vote(
    ctx: &Context,
    interaction: &Interaction,
    interaction_id: InteractionId,
    component: &MessageComponent,
) -> anyhow::Result<()> {
    // save the user's vote in the poll data
    let mut lock = POLLS.write().await;
    let poll_data = lock
        .get_mut(&interaction_id)
        .context("unexpected interaction id")?;
    let user_id = interaction
        .member
        .as_ref()
        .context("missing member")?
        .user
        .id;
    poll_data.votes.insert(user_id, component.custom_id.clone());

    // update the message
    interaction
        .create_interaction_response(ctx, |response| {
            response
                .kind(InteractionResponseType::UpdateMessage)
                .interaction_response_data(|response_data| {
                    response_data.content(create_content(&poll_data))
                })
        })
        .await?;
    Ok(())
}

/// Periodically removes old poll data from memory
pub async fn cleaner(interval: Duration, poll_duration: Duration) {
    let mut interval = tokio::time::interval(interval);
    loop {
        interval.tick().await;
        let mut lock = POLLS.write().await;
        let mut remove = vec![];
        for (key, val) in lock.iter() {
            if val.start_time.elapsed() > poll_duration {
                remove.push(*key);
            }
        }
        for target in remove {
            lock.remove(&target);
        }
    }
}

fn create_content(poll_data: &PollData) -> String {
    format!("Vote:\n{}", poll_data)
}

static POLLS: Lazy<RwLock<HashMap<InteractionId, PollData>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

struct PollData {
    start_time: Instant,
    options: Vec<String>,
    votes: HashMap<UserId, String>,
}

impl Display for PollData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut vote_map = BTreeMap::<&str, u32>::new();
        for option in &self.options {
            vote_map.insert(option, 0);
        }
        for (_user, option) in self.votes.iter() {
            if let Some(votes) = vote_map.get_mut(option.as_str()) {
                *votes += 1;
            }
        }
        for (option, votes) in vote_map {
            write!(f, "{}: {}\n", option, votes)?;
        }
        Ok(())
    }
}
