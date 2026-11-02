-- A file's dynamic range, beside its width and height: every library
-- listing badges its posters with it, and reading it out of media_info
-- for each file of a library is slow. Scans write it; files scanned before
-- take it from media_info once.
ALTER TABLE media_files ADD COLUMN dynamic_range TEXT;
UPDATE media_files SET dynamic_range = (
    SELECT json_extract(s.value, '$.dynamic_range') FROM json_each(media_info, '$.streams') s
    WHERE json_extract(s.value, '$.kind') = 'video' LIMIT 1)
WHERE json_valid(media_info);
