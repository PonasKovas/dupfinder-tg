use crate::{config::Settings, database};
use anyhow::Result;
use img_hash::HasherConfig;
use sqlx::PgPool;
use std::io::Cursor;
use teloxide::{net::Download, prelude::*, sugar::request::RequestReplyExt, types::MessageId};
use tracing::{debug, error};

#[derive(Clone)]
struct BotState {
    settings: Settings,
    pool: PgPool,
}

pub async fn run(settings: Settings, pool: PgPool) -> Result<()> {
    let bot = Bot::new(settings.telegram.token.clone());

    let state = BotState { pool, settings };

    // Define the command handler (or message handler)
    let handler = Update::filter_message().endpoint(message_handler);

    println!("Bot started...");

    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![state])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

    Ok(())
}

async fn message_handler(bot: Bot, msg: Message, state: BotState) -> ResponseResult<()> {
    let chat_id = msg.chat.id.0;
    let message_id = msg.id.0;
    let title = msg
        .chat
        .title()
        .or(msg.chat.username())
        .unwrap_or("<unknown>");

    if let Some("duplicate?" | "dup?") = msg.text()
        && let Some(referenced_msg) = msg.reply_to_message()
    {
        let hash = match get_img_hash(&bot, &referenced_msg).await? {
            Some(x) => x,
            None => {
                return Ok(());
            }
        };

        match database::find_closest_match(
            &state.pool,
            chat_id,
            hash,
            64, // maximum bits in i64
            Some(referenced_msg.id.0),
        )
        .await
        {
            Ok(Some(closest_match)) => {
                bot.send_message(
                    msg.chat.id,
                    format!(
                        "closest match (dst {distance}).",
                        distance = closest_match.distance
                    ),
                )
                .reply_to(MessageId(closest_match.message_id))
                .await?;
            }
            Ok(None) => {}
            Err(e) => {
                error!("Database error: {e}");
                return Ok(());
            }
        };
    }

    let hash = match get_img_hash(&bot, &msg).await? {
        Some(x) => x,
        None => {
            return Ok(());
        }
    };

    let result = match database::find_closest_match(
        &state.pool,
        chat_id,
        hash,
        state.settings.similarity_threshold,
        None,
    )
    .await
    {
        Ok(x) => x,
        Err(e) => {
            error!("Database error: {e}");
            return Ok(());
        }
    };

    match result {
        Some(closest_match) => {
            bot.send_message(
                msg.chat.id,
                format!(
                    "duplicate image (dst {distance}).",
                    distance = closest_match.distance
                ),
            )
            .reply_to(MessageId(closest_match.message_id))
            .await?;
        }
        None => {
            debug!("new image sent to {title} ({chat_id}). adding hash to memory");

            match database::save_image(&state.pool, chat_id, title, message_id, hash).await {
                Ok(()) => (),
                Err(e) => {
                    error!("Database error: {e}");
                    return Ok(());
                }
            }
        }
    }

    Ok(())
}

async fn get_img_hash(bot: &Bot, msg: &Message) -> ResponseResult<Option<i64>> {
    // Try to extract the file_id
    let file_id = if let Some(photos) = msg.photo() {
        // It's a compressed photo (take the largest)
        // We can unwrap safe because the vector is never empty if the field is Some
        Some(photos.last().unwrap().file.id.clone())
    } else if let Some(doc) = msg.document() {
        // It's a file/document. Check if it's an image.
        if let Some(mime) = &doc.mime_type {
            if mime.type_() == mime::IMAGE {
                Some(doc.file.id.clone())
            } else {
                None // It is a document, but not an image (e.g. PDF)
            }
        } else {
            None // Unknown mime type
        }
    } else {
        // not photo nor document
        None
    };

    let file_id = match file_id {
        Some(id) => id,
        None => return Ok(None), // Not an image? Ignore and exit.
    };

    debug!("Downloading {file_id}...");
    let file_info = bot.get_file(file_id).await?;

    let mut image_data = Vec::new();
    bot.download_file(&file_info.path, &mut image_data).await?;

    let hash = match calculate_hash(image_data.as_slice()) {
        Ok(x) => x,
        Err(e) => {
            error!(
                "Error decoding image (msg id: {message_id}) in {title:?} ({chat_id}): {e}",
                message_id = msg.id.0,
                title = msg.chat.title().or(msg.chat.username()),
                chat_id = msg.chat.id.0,
            );
            return Ok(None);
        }
    };

    Ok(Some(hash))
}

fn calculate_hash(image: &[u8]) -> Result<i64, image::ImageError> {
    let image = image::io::Reader::new(Cursor::new(image))
        .with_guessed_format()?
        .decode()?;
    let hasher = HasherConfig::new().to_hasher();

    let hash = hasher.hash_image(&image);

    let Ok(hash): Result<[u8; 8], _> = hash.as_bytes().try_into() else {
        panic!("Hash was not exactly 8 bytes!");
    };
    let hash = i64::from_be_bytes(hash);

    Ok(hash)
}
