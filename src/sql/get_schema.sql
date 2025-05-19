SELECT sql FROM sqlite_master 
WHERE type IN ('table', 'index') 
AND name NOT LIKE 'sqlite_%'
ORDER BY type, name