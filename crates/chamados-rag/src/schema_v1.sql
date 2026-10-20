CREATE TABLE documents (
    id            INTEGER PRIMARY KEY,
    name          TEXT NOT NULL,
    path          TEXT NOT NULL,
    kind          TEXT NOT NULL,
    size          INTEGER NOT NULL,
    state         TEXT NOT NULL,
    pages         INTEGER,
    indexed_pages INTEGER NOT NULL DEFAULT 0,
    chunks        INTEGER NOT NULL DEFAULT 0,
    added_at      INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL
);

CREATE TABLE chunks (
    id      INTEGER PRIMARY KEY,
    doc_id  INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    page    INTEGER,
    ordinal INTEGER NOT NULL,
    text    TEXT NOT NULL
);

CREATE INDEX chunks_doc ON chunks(doc_id);

-- Busca por palavras, sem diferenciar maiúsculas nem acentos; o texto fica na tabela `chunks`.
CREATE VIRTUAL TABLE chunk_fts USING fts5(
    text,
    content = 'chunks',
    content_rowid = 'id',
    tokenize = 'unicode61 remove_diacritics 2'
);

CREATE TRIGGER chunks_after_insert AFTER INSERT ON chunks BEGIN
    INSERT INTO chunk_fts(rowid, text) VALUES (new.id, new.text);
END;

CREATE TRIGGER chunks_after_delete AFTER DELETE ON chunks BEGIN
    INSERT INTO chunk_fts(chunk_fts, rowid, text) VALUES ('delete', old.id, old.text);
END;
