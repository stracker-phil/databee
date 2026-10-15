-- WordPress schemas use zero-date defaults, which strict mode rejects.
SET SESSION sql_mode = '';

CREATE DATABASE databee_test CHARACTER SET utf8mb4;
USE databee_test;

CREATE TABLE wp_posts (
  ID bigint(20) unsigned NOT NULL AUTO_INCREMENT,
  post_author bigint(20) unsigned NOT NULL DEFAULT 0,
  post_date datetime NOT NULL DEFAULT '0000-00-00 00:00:00',
  post_title text NOT NULL,
  post_status varchar(20) NOT NULL DEFAULT 'publish',
  post_type varchar(20) NOT NULL DEFAULT 'post',
  PRIMARY KEY (ID),
  KEY type_status_date (post_type, post_status, post_date, ID)
) COMMENT='Posts';

INSERT INTO wp_posts (post_author, post_date, post_title, post_status, post_type) VALUES
  (1, '2026-01-01 10:00:00', 'Hello world', 'publish', 'post'),
  (1, '2026-01-02 11:00:00', 'Sample Page', 'publish', 'page'),
  (2, '2026-01-03 12:00:00', 'Draft', 'draft', 'post');

CREATE TABLE types (
  id int NOT NULL PRIMARY KEY,
  big bigint unsigned,
  price decimal(10,2),
  ratio double,
  flag tinyint(1),
  created datetime,
  zero_date datetime,
  payload json,
  blob_data blob,
  bits bit(8),
  label varchar(50) COLLATE utf8mb4_bin
);

INSERT INTO types VALUES
  (1, 18446744073709551615, 49.90, 0.5, 1, '2026-10-15 20:00:00', '0000-00-00 00:00:00',
   '{"a": 1}', UNHEX('00FF10'), b'00000101', 'bin text'),
  (2, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL);

CREATE VIEW published AS SELECT ID, post_title FROM wp_posts WHERE post_status = 'publish';
