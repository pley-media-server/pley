-- Probes now look in the first frames of 10-bit HEVC video for Dolby Vision
-- the file doesn't name (no configuration record, as some remuxes and older
-- muxers leave it): such files were taken for their base layer, HDR10 or
-- HDR10+ mostly. The next scan probes these files again to find it.
UPDATE media_files SET probed = 0
WHERE probed = 1 AND media_info IS NOT NULL AND json_valid(media_info)
  AND EXISTS (
    SELECT 1 FROM json_each(media_info, '$.streams') s
    WHERE json_extract(s.value, '$.kind') = 'video'
      AND json_extract(s.value, '$.codec') = 'hevc'
      AND COALESCE(json_extract(s.value, '$.dynamic_range'), 'sdr') != 'dolby_vision'
      AND COALESCE(json_extract(s.value, '$.bit_depth'), 10) >= 10
  );
