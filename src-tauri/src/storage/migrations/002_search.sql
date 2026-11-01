CREATE VIRTUAL TABLE words_fts USING fts5(search_text, content='words', content_rowid='rowid', tokenize='trigram');
CREATE TRIGGER words_search_insert AFTER INSERT ON words BEGIN INSERT INTO words_fts(rowid,search_text) VALUES(new.rowid,new.search_text); END;
CREATE TRIGGER words_search_delete AFTER DELETE ON words BEGIN INSERT INTO words_fts(words_fts,rowid,search_text) VALUES('delete',old.rowid,old.search_text); END;
CREATE TRIGGER words_search_update AFTER UPDATE OF search_text ON words BEGIN INSERT INTO words_fts(words_fts,rowid,search_text) VALUES('delete',old.rowid,old.search_text); INSERT INTO words_fts(rowid,search_text) VALUES(new.rowid,new.search_text); END;
INSERT INTO words_fts(words_fts) VALUES('rebuild');
