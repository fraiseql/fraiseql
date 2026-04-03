-- FraiseQL MySQL Stored Procedures
--
-- Loaded separately from init.sql because DELIMITER is a client-side
-- directive that does not work reliably when piped via stdin.
-- CI loads this file with: mysql --delimiter="//" … < procedures.sql

DROP PROCEDURE IF EXISTS fn_create_tag//
CREATE PROCEDURE fn_create_tag(IN p_name VARCHAR(200))
BEGIN
    INSERT INTO tb_tag (name) VALUES (p_name)
      ON DUPLICATE KEY UPDATE pk_tag = LAST_INSERT_ID(pk_tag), name = p_name;
    SELECT pk_tag AS id, name FROM tb_tag WHERE pk_tag = LAST_INSERT_ID();
END //
