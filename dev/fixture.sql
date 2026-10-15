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

-- Minimal WordPress + WooCommerce (HPOS with sync) install for the WP lens tests.
CREATE DATABASE wp_test CHARACTER SET utf8mb4;
USE wp_test;

CREATE TABLE wp_options (option_id bigint unsigned AUTO_INCREMENT PRIMARY KEY, option_name varchar(191) UNIQUE, option_value longtext, autoload varchar(20) DEFAULT 'yes');
CREATE TABLE wp_users (ID bigint unsigned AUTO_INCREMENT PRIMARY KEY, user_login varchar(60), user_pass varchar(255), user_email varchar(100), display_name varchar(250));
CREATE TABLE wp_usermeta (umeta_id bigint unsigned AUTO_INCREMENT PRIMARY KEY, user_id bigint unsigned, meta_key varchar(255), meta_value longtext);
CREATE TABLE wp_posts (ID bigint unsigned AUTO_INCREMENT PRIMARY KEY, post_author bigint unsigned DEFAULT 0, post_title text, post_status varchar(20), post_type varchar(20), post_parent bigint unsigned DEFAULT 0);
CREATE TABLE wp_postmeta (meta_id bigint unsigned AUTO_INCREMENT PRIMARY KEY, post_id bigint unsigned, meta_key varchar(255), meta_value longtext);
CREATE TABLE wp_comments (comment_ID bigint unsigned AUTO_INCREMENT PRIMARY KEY, comment_post_ID bigint unsigned, comment_author varchar(255), comment_content text, comment_type varchar(20), comment_parent bigint unsigned DEFAULT 0, user_id bigint unsigned DEFAULT 0);
CREATE TABLE wp_commentmeta (meta_id bigint unsigned AUTO_INCREMENT PRIMARY KEY, comment_id bigint unsigned, meta_key varchar(255), meta_value longtext);
CREATE TABLE wp_wc_orders (id bigint unsigned PRIMARY KEY, status varchar(20), type varchar(20), currency varchar(10), total_amount decimal(26,8), customer_id bigint unsigned, billing_email varchar(320), parent_order_id bigint unsigned DEFAULT 0);
CREATE TABLE wp_wc_orders_meta (id bigint unsigned AUTO_INCREMENT PRIMARY KEY, order_id bigint unsigned, meta_key varchar(255), meta_value text);
CREATE TABLE wp_wc_order_addresses (id bigint unsigned AUTO_INCREMENT PRIMARY KEY, order_id bigint unsigned, address_type varchar(20), email varchar(320), country varchar(2));
CREATE TABLE wp_wc_order_operational_data (id bigint unsigned AUTO_INCREMENT PRIMARY KEY, order_id bigint unsigned, created_via varchar(100));
CREATE TABLE wp_woocommerce_order_items (order_item_id bigint unsigned AUTO_INCREMENT PRIMARY KEY, order_item_name text, order_item_type varchar(200), order_id bigint unsigned);
CREATE TABLE wp_woocommerce_order_itemmeta (meta_id bigint unsigned AUTO_INCREMENT PRIMARY KEY, order_item_id bigint unsigned, meta_key varchar(255), meta_value longtext);
CREATE TABLE wp_actionscheduler_actions (action_id bigint unsigned AUTO_INCREMENT PRIMARY KEY, hook varchar(191));

INSERT INTO wp_options (option_name, option_value, autoload) VALUES
  ('siteurl', 'https://shop.test', 'yes'),
  ('db_version', '58975', 'yes'),
  ('woocommerce_version', '9.3.0', 'yes'),
  ('woocommerce_custom_orders_table_enabled', 'yes', 'yes'),
  ('woocommerce_custom_orders_table_data_sync_enabled', 'yes', 'yes'),
  ('woocommerce_ppcp-settings', 'a:2:{s:7:"enabled";s:3:"yes";s:5:"title";s:6:"PayPal";}', 'yes'),
  ('_transient_old_cache', 'x', 'no'),
  ('_transient_timeout_old_cache', '1000', 'no'),
  ('_transient_fresh_cache', 'y', 'no'),
  ('_transient_timeout_fresh_cache', '4102444800', 'no');

INSERT INTO wp_users (ID, user_login, user_pass, user_email, display_name) VALUES (12, 'jane', '$P$secret', 'jane@shop.test', 'Jane Doe');
INSERT INTO wp_usermeta (user_id, meta_key, meta_value) VALUES (12, 'billing_country', 'AT');
INSERT INTO wp_posts (ID, post_author, post_title, post_status, post_type) VALUES (123, 12, 'Mug', 'publish', 'product');
INSERT INTO wp_postmeta (post_id, meta_key, meta_value) VALUES (123, '_price', '24.95'), (123, '_sku', 'MUG-1');

INSERT INTO wp_wc_orders VALUES (4711, 'wc-processing', 'shop_order', 'EUR', 49.90, 12, 'jane@shop.test', 0);
INSERT INTO wp_wc_orders VALUES (4712, 'wc-refunded', 'shop_order_refund', 'EUR', -10.00, 0, NULL, 4711);
INSERT INTO wp_wc_orders_meta (order_id, meta_key, meta_value) VALUES
  (4711, '_ppcp_paypal_order_id', '8XY12345'),
  (4711, '_ppcp_paypal_intent', 'CAPTURE'),
  (4711, '_payment_method', 'ppcp-gateway');
INSERT INTO wp_wc_order_addresses (order_id, address_type, email, country) VALUES (4711, 'billing', 'jane@shop.test', 'AT'), (4711, 'shipping', NULL, 'AT');
INSERT INTO wp_wc_order_operational_data (order_id, created_via) VALUES (4711, 'checkout');
INSERT INTO wp_woocommerce_order_items (order_item_id, order_item_name, order_item_type, order_id) VALUES (1, 'Mug', 'line_item', 4711), (2, 'Flat rate', 'shipping', 4711);
INSERT INTO wp_woocommerce_order_itemmeta (order_item_id, meta_key, meta_value) VALUES (1, '_product_id', '123'), (1, '_qty', '2');
INSERT INTO wp_comments (comment_post_ID, comment_author, comment_content, comment_type) VALUES
  (4711, 'WooCommerce', 'Payment captured', 'order_note'),
  (4711, 'Jane', 'Nice mug', 'review');
