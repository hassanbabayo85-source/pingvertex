DO $$
BEGIN
    -- Farko, mu duba ko akwai wani tsohon constraint da sunan da aka saba amfani da shi ko wani wanda ke iyakance status
    IF NOT EXISTS (
        SELECT 1 
        FROM pg_constraint c
        JOIN pg_class r ON r.oid = c.conrelid
        WHERE r.relname = 'wallet_auto_top_up_attempts' 
          AND c.contype = 'c'
          AND pg_get_constraintdef(c.oid) ILIKE '%status%SUCCEEDED%'
          AND pg_get_constraintdef(c.oid) ILIKE '%PROCESSING%'
    ) THEN
        -- Idan babu sabon tsarin a ciki, za mu goge duk wani constraint na status na tsohon nauyi
        DECLARE
            r_con RECORD;
        BEGIN
            FOR r_con IN 
                SELECT conname 
                FROM pg_constraint c
                JOIN pg_class cl ON cl.oid = c.conrelid
                WHERE cl.relname = 'wallet_auto_top_up_attempts' 
                  AND c.contype = 'c'
                  AND pg_get_constraintdef(c.oid) ILIKE '%status%'
            LOOP
                EXECUTE 'ALTER TABLE wallet_auto_top_up_attempts DROP CONSTRAINT ' || quote_ident(r_con.conname);
            END LOOP;
        END;

        -- Sannan mu ƙara sabon constraint ɗin da duk statuses ɗin
        ALTER TABLE wallet_auto_top_up_attempts
        ADD CONSTRAINT wallet_auto_top_up_attempts_status_check
        CHECK (status IN ('PENDING', 'PROCESSING', 'SUCCEEDED', 'FAILED', 'REQUIRES_RECONCILIATION'));
    END IF;
END $$;
