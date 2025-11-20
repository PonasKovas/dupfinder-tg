CREATE TABLE chats (
    id BIGINT PRIMARY KEY,
    title TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE images (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    chat_id BIGINT NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
    message_id INTEGER NOT NULL,
    phash BIGINT NOT NULL
);
