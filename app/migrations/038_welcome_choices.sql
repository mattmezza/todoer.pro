-- Save the small welcome draft without creating entities between steps.
ALTER TABLE user_onboarding ADD COLUMN spaces TEXT NOT NULL DEFAULT '[]';
