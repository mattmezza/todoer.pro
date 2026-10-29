-- Trusted provider picture, refreshed from verified Google ID-token claims.
ALTER TABLE auth_identities ADD COLUMN picture_url TEXT;
