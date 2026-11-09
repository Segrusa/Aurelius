-- How many model calls a usage record covers (U4, `TokenUsage::model_calls`).
-- Records from before this migration stay 0: that number is not known.
ALTER TABLE usage_records ADD COLUMN model_calls INTEGER NOT NULL DEFAULT 0;
