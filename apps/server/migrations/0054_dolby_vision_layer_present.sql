-- Probes now record whether a Dolby Vision profile 7 stream carries its
-- enhancement layer (its configuration record's el_present_flag): the
-- experimental FEL rebuild is made only where it does. Files with a full
-- layer probed before don't say, so the next scan probes them again.
UPDATE media_files SET probed = 0
WHERE probed = 1 AND media_info IS NOT NULL AND json_valid(media_info)
  AND EXISTS (
    SELECT 1 FROM json_each(media_info, '$.streams') s
    WHERE json_extract(s.value, '$.kind') = 'video'
      AND json_extract(s.value, '$.dv_profile') = 7
      AND json_extract(s.value, '$.dv_el') = 'fel'
      AND json_extract(s.value, '$.dv_el_present') IS NULL
  );
