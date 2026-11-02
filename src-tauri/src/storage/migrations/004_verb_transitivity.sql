ALTER TABLE word_pos RENAME TO word_pos_previous;
CREATE TABLE word_pos(word_id TEXT NOT NULL REFERENCES words(id) ON DELETE CASCADE, pos_code TEXT NOT NULL CHECK(pos_code IN ('noun','godan_verb','ichidan_verb','suru_verb','kuru_verb','i_adjective','na_adjective','adverb','particle','expression','other','intransitive','transitive')), PRIMARY KEY(word_id,pos_code));
INSERT INTO word_pos SELECT word_id,pos_code FROM word_pos_previous;
DROP TABLE word_pos_previous;
