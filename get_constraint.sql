SELECT conname 
FROM pg_constraint 
WHERE conrelid = 'wallet_auto_top_up_attempts'::regclass 
AND contype = 'c';
