ALTER TABLE "order_outbox_messages"
ADD COLUMN IF NOT EXISTS "LockedUntilUtc" timestamptz NULL;

ALTER TABLE "inventory_outbox_messages"
ADD COLUMN IF NOT EXISTS "LockedUntilUtc" timestamptz NULL;

CREATE INDEX IF NOT EXISTS "IX_order_outbox_messages_publish_lock"
ON "order_outbox_messages" ("PublishedOnUtc", "LockedUntilUtc", "Id");

CREATE INDEX IF NOT EXISTS "IX_inventory_outbox_messages_publish_lock"
ON "inventory_outbox_messages" ("PublishedOnUtc", "LockedUntilUtc", "Id");
