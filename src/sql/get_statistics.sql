SELECT 
    COUNT(*) as total_messages,
    COUNT(DISTINCT user_id) as unique_users,
    MIN(timestamp) as first_message_time,
    MAX(timestamp) as last_message_time
FROM messages