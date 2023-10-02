BEGIN;

-- users
CREATE TABLE IF NOT EXISTS users(
    uuid UUID PRIMARY KEY,
    sid VARCHAR NOT NULL UNIQUE,
    name TEXT,
    email TEXT NOT NULL,
    password TEXT NOT NULL,
    disabled INTEGER NOT NULL DEFAULT 0,
    confirm_code VARCHAR NOT NULL UNIQUE,
    confirmed TIMESTAMP,
    last_login TIMESTAMP,
    invited_by UUID,
    created TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated TIMESTAMP
);
CREATE UNIQUE INDEX IF NOT EXISTS ix_users_email ON users(email);
CREATE INDEX IF NOT EXISTS ix_users_sid ON users(sid);
CREATE INDEX IF NOT EXISTS ix_users_password ON users(password);
CREATE INDEX IF NOT EXISTS ix_users_confirm_code ON users(confirm_code);
CREATE INDEX IF NOT EXISTS ix_users_confirmed ON users(confirmed);

-- lists
CREATE TABLE IF NOT EXISTS lists(
    uuid UUID PRIMARY KEY,
    sid VARCHAR NOT NULL UNIQUE,
    title VARCHAR NOT NULL,
    descr TEXT,
    color VARCHAR NOT NULL DEFAULT 'FF0000',
    creator UUID NOT NULL REFERENCES users(uuid) ON UPDATE CASCADE ON DELETE CASCADE,
    created TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated TIMESTAMP
);
CREATE INDEX IF NOT EXISTS ix_lists_sid ON lists(sid);
CREATE INDEX IF NOT EXISTS ix_lists_title ON lists(title);
CREATE INDEX IF NOT EXISTS ix_lists_creator ON lists(creator);

-- sections
CREATE TABLE IF NOT EXISTS sections(
    uuid UUID PRIMARY KEY,
    sid VARCHAR NOT NULL UNIQUE,
    `order` INTEGER NOT NULL,
    title VARCHAR NOT NULL,
    descr TEXT,
    creator UUID NOT NULL REFERENCES users(uuid) ON UPDATE CASCADE ON DELETE CASCADE,
    created TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated TIMESTAMP,
    list UUID NOT NULL REFERENCES lists(uuid) ON UPDATE CASCADE ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS ix_sections_sid ON sections(sid);
CREATE INDEX IF NOT EXISTS ix_sections_order ON sections(`order`);
CREATE INDEX IF NOT EXISTS ix_sections_title ON sections(title);
CREATE INDEX IF NOT EXISTS ix_sections_creator ON sections(creator);
CREATE INDEX IF NOT EXISTS ix_sections_list ON sections(list);

-- tasks
CREATE TABLE IF NOT EXISTS tasks(
    uuid UUID PRIMARY KEY,
    sid VARCHAR NOT NULL UNIQUE,
    `order` INTEGER NOT NULL,
    title VARCHAR(255) NOT NULL,
    descr TEXT,
    complete INTEGER NOT NULL DEFAULT 0,
    completed TIMESTAMP,
    created TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated TIMESTAMP,
    section UUID REFERENCES sections(uuid) ON UPDATE CASCADE ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS ix_tasks_sid ON tasks(sid);
CREATE INDEX IF NOT EXISTS ix_tasks_order ON tasks(`order`);
CREATE INDEX IF NOT EXISTS ix_tasks_title ON tasks(title);
CREATE INDEX IF NOT EXISTS ix_tasks_complete ON tasks(complete);
CREATE INDEX IF NOT EXISTS ix_tasks_section ON tasks(section);

-- subtasks
CREATE TABLE IF NOT EXISTS subtasks(
    uuid UUID PRIMARY KEY,
    sid VARCHAR NOT NULL UNIQUE,
    `order` INTEGER NOT NULL,
    title VARCHAR(255) NOT NULL,
    descr TEXT,
    complete INTEGER NOT NULL DEFAULT 0,
    completed TIMESTAMP,
    created TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated TIMESTAMP,
    task UUID REFERENCES tasks(uuid) ON UPDATE CASCADE ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS ix_subtasks_sid ON subtasks(sid);
CREATE INDEX IF NOT EXISTS ix_subtasks_order ON subtasks(`order`);
CREATE INDEX IF NOT EXISTS ix_subtasks_title ON subtasks(title);
CREATE INDEX IF NOT EXISTS ix_subtasks_complete ON subtasks(complete);
CREATE INDEX IF NOT EXISTS ix_subtasks_task ON subtasks(task);

-- comments
CREATE TABLE IF NOT EXISTS comments(
    uuid UUID PRIMARY KEY,
    sid VARCHAR NOT NULL UNIQUE,
    content TEXT,
    author UUID NOT NULL REFERENCES users(uuid) ON UPDATE CASCADE ON DELETE SET NULL,
    created TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated TIMESTAMP,
    task UUID REFERENCES tasks(uuid) ON UPDATE CASCADE ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS ix_comments_sid ON comments(sid);
CREATE INDEX IF NOT EXISTS ix_comments_author ON comments(author);
CREATE INDEX IF NOT EXISTS ix_comments_task ON comments(task);

-- attachments
CREATE TABLE IF NOT EXISTS attachments(
    uuid UUID PRIMARY KEY,
    sid VARCHAR NOT NULL UNIQUE,
    mime VARCHAR NOT NULL,
    fname VARCHAR NOT NULL,
    size INTEGER NOT NULL,
    url VARCHAR NOT NULL,
    created TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    task UUID REFERENCES tasks(uuid) ON UPDATE CASCADE ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS ix_attachments_sid ON attachments(sid);
CREATE INDEX IF NOT EXISTS ix_attachments_task ON attachments(task);

COMMIT;
