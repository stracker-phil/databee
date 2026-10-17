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

-- A real foreign key, which WordPress tables never declare.
CREATE TABLE authors (
  id int NOT NULL PRIMARY KEY,
  name varchar(50) NOT NULL
) ENGINE=InnoDB;

CREATE TABLE books (
  id int NOT NULL PRIMARY KEY,
  author_id int,
  title varchar(100) NOT NULL,
  KEY author_id (author_id),
  CONSTRAINT books_author FOREIGN KEY (author_id) REFERENCES authors (id)
) ENGINE=InnoDB;

INSERT INTO authors VALUES (1, 'Ada'), (2, 'Grace');
INSERT INTO books VALUES (1, 1, 'Notes'), (2, 2, 'Compilers'), (3, NULL, 'Anonymous');

-- Reference coverage: one resolved child, a zero and a NULL (no reference), two orphans.
-- No declared foreign key, so the same-name inference finds the reference.
CREATE TABLE cov_parents (
  parent_id int NOT NULL PRIMARY KEY
) ENGINE=InnoDB;

CREATE TABLE cov_children (
  child_id int NOT NULL PRIMARY KEY,
  parent_id int,
  KEY parent_id (parent_id)
) ENGINE=InnoDB;

INSERT INTO cov_parents VALUES (1), (2);
INSERT INTO cov_children VALUES (1, 1), (2, 0), (3, NULL), (4, 98), (5, 99);

-- WordPress + WooCommerce (HPOS with sync) install for the WP lens tests.
CREATE DATABASE wp_test CHARACTER SET utf8mb4;
USE wp_test;

CREATE TABLE wp_options (option_id bigint unsigned AUTO_INCREMENT PRIMARY KEY, option_name varchar(191) UNIQUE, option_value longtext, autoload varchar(20) DEFAULT 'yes');
CREATE TABLE wp_users (ID bigint unsigned AUTO_INCREMENT PRIMARY KEY, user_login varchar(60), user_pass varchar(255), user_email varchar(100), user_registered datetime DEFAULT '0000-00-00 00:00:00', display_name varchar(250), user_activation_key varchar(255) NOT NULL DEFAULT '');
CREATE TABLE wp_usermeta (umeta_id bigint unsigned AUTO_INCREMENT PRIMARY KEY, user_id bigint unsigned, meta_key varchar(255), meta_value longtext);
CREATE TABLE wp_posts (ID bigint unsigned AUTO_INCREMENT PRIMARY KEY, post_author bigint unsigned DEFAULT 0, post_date datetime NOT NULL DEFAULT '0000-00-00 00:00:00', post_date_gmt datetime DEFAULT '0000-00-00 00:00:00', post_content longtext NOT NULL, post_title text, post_excerpt text, post_status varchar(20), post_name varchar(200) DEFAULT '', post_modified datetime DEFAULT '0000-00-00 00:00:00', post_modified_gmt datetime DEFAULT '0000-00-00 00:00:00', post_parent bigint unsigned DEFAULT 0, guid varchar(255) NOT NULL DEFAULT '', menu_order int DEFAULT 0, post_type varchar(20));
CREATE TABLE wp_postmeta (meta_id bigint unsigned AUTO_INCREMENT PRIMARY KEY, post_id bigint unsigned, meta_key varchar(255), meta_value longtext);
CREATE TABLE wp_comments (comment_ID bigint unsigned AUTO_INCREMENT PRIMARY KEY, comment_post_ID bigint unsigned, comment_author varchar(255), comment_date_gmt datetime DEFAULT '0000-00-00 00:00:00', comment_content text, comment_type varchar(20), comment_parent bigint unsigned DEFAULT 0, user_id bigint unsigned DEFAULT 0, comment_author_email varchar(100) NOT NULL DEFAULT '', comment_date datetime NOT NULL DEFAULT '0000-00-00 00:00:00', comment_approved varchar(20) NOT NULL DEFAULT '1');
CREATE TABLE wp_commentmeta (meta_id bigint unsigned AUTO_INCREMENT PRIMARY KEY, comment_id bigint unsigned, meta_key varchar(255), meta_value longtext);
CREATE TABLE wp_terms (term_id bigint unsigned AUTO_INCREMENT PRIMARY KEY, name varchar(200), slug varchar(200));
CREATE TABLE wp_term_taxonomy (term_taxonomy_id bigint unsigned AUTO_INCREMENT PRIMARY KEY, term_id bigint unsigned, taxonomy varchar(32), parent bigint unsigned NOT NULL DEFAULT 0);
CREATE TABLE wp_term_relationships (object_id bigint unsigned, term_taxonomy_id bigint unsigned, term_order int NOT NULL DEFAULT 0, PRIMARY KEY (object_id, term_taxonomy_id));
CREATE TABLE wp_wc_orders (id bigint unsigned PRIMARY KEY, status varchar(20), type varchar(20), currency varchar(10), total_amount decimal(26,8), customer_id bigint unsigned, billing_email varchar(320), date_created_gmt datetime, date_updated_gmt datetime, parent_order_id bigint unsigned DEFAULT 0, payment_method varchar(100), payment_method_title text, transaction_id varchar(100), tax_amount decimal(26,8));
CREATE TABLE wp_wc_orders_meta (id bigint unsigned AUTO_INCREMENT PRIMARY KEY, order_id bigint unsigned, meta_key varchar(255), meta_value text);
CREATE TABLE wp_wc_order_addresses (id bigint unsigned AUTO_INCREMENT PRIMARY KEY, order_id bigint unsigned, address_type varchar(20), first_name text, last_name text, company text, address_1 text, address_2 text, city text, state text, postcode text, country text, email varchar(320), phone varchar(100));
CREATE TABLE wp_wc_order_operational_data (id bigint unsigned AUTO_INCREMENT PRIMARY KEY, order_id bigint unsigned, created_via varchar(100), date_paid_gmt datetime NULL, date_completed_gmt datetime NULL, order_key varchar(100) NULL, shipping_tax_amount decimal(26,8) NULL, shipping_total_amount decimal(26,8) NULL, discount_tax_amount decimal(26,8) NULL, discount_total_amount decimal(26,8) NULL);
CREATE TABLE wp_woocommerce_order_items (order_item_id bigint unsigned AUTO_INCREMENT PRIMARY KEY, order_item_name text, order_item_type varchar(200), order_id bigint unsigned);
CREATE TABLE wp_woocommerce_order_itemmeta (meta_id bigint unsigned AUTO_INCREMENT PRIMARY KEY, order_item_id bigint unsigned, meta_key varchar(255), meta_value longtext);
CREATE TABLE wp_wc_product_meta_lookup (product_id bigint PRIMARY KEY, sku varchar(100), min_price decimal(19,4), max_price decimal(19,4), stock_quantity double, stock_status varchar(100), total_sales bigint DEFAULT 0, has_price_range tinyint(1) GENERATED ALWAYS AS (min_price <> max_price) STORED);
CREATE TABLE wp_wc_order_product_lookup (order_item_id bigint unsigned PRIMARY KEY, order_id bigint unsigned, product_id bigint unsigned, variation_id bigint unsigned, customer_id bigint unsigned NULL, product_net_revenue double NOT NULL DEFAULT 0, product_gross_revenue double NOT NULL DEFAULT 0);
CREATE TABLE wp_actionscheduler_actions (action_id bigint unsigned AUTO_INCREMENT PRIMARY KEY, hook varchar(191), status varchar(20), scheduled_date_gmt datetime DEFAULT '0000-00-00 00:00:00', scheduled_date_local datetime DEFAULT '0000-00-00 00:00:00', priority tinyint unsigned NOT NULL DEFAULT 10, args varchar(191), group_id bigint unsigned DEFAULT 0, attempts int DEFAULT 0, claim_id bigint unsigned DEFAULT 0, last_attempt_gmt datetime DEFAULT '0000-00-00 00:00:00', last_attempt_local datetime DEFAULT '0000-00-00 00:00:00', extended_args varchar(8000) DEFAULT NULL);
CREATE TABLE wp_actionscheduler_groups (group_id bigint unsigned AUTO_INCREMENT PRIMARY KEY, slug varchar(255));
CREATE TABLE wp_actionscheduler_logs (log_id bigint unsigned AUTO_INCREMENT PRIMARY KEY, action_id bigint unsigned, message text, log_date_gmt datetime, log_date_local datetime DEFAULT '0000-00-00 00:00:00');
CREATE TABLE wp_woocommerce_sessions (session_id bigint unsigned AUTO_INCREMENT PRIMARY KEY, session_key char(32) UNIQUE, session_value longtext, session_expiry bigint unsigned);
CREATE TABLE wp_woocommerce_log (log_id bigint unsigned AUTO_INCREMENT PRIMARY KEY, timestamp datetime, level smallint, source varchar(200), message longtext, context longtext);
CREATE TABLE wp_woocommerce_payment_tokens (token_id bigint unsigned AUTO_INCREMENT PRIMARY KEY, gateway_id varchar(200), token text, user_id bigint unsigned DEFAULT 0, type varchar(200), is_default tinyint(1) DEFAULT 0);
CREATE TABLE wp_woocommerce_payment_tokenmeta (meta_id bigint unsigned AUTO_INCREMENT PRIMARY KEY, payment_token_id bigint unsigned, meta_key varchar(255), meta_value longtext);

INSERT INTO wp_options (option_name, option_value, autoload) VALUES
  ('siteurl', 'https://shop.test', 'yes'),
  ('blogname', 'Test Shop', 'yes'),
  ('db_version', '58975', 'yes'),
  ('woocommerce_version', '9.3.0', 'yes'),
  ('woocommerce_currency', 'EUR', 'yes'),
  ('gmt_offset', '2', 'yes'),
  ('timezone_string', 'Europe/Berlin', 'yes'),
  ('stylesheet', 'storefront', 'yes'),
  ('active_plugins', 'a:1:{i:0;s:27:"woocommerce/woocommerce.php";}', 'yes'),
  ('woocommerce_custom_orders_table_enabled', 'yes', 'yes'),
  ('woocommerce_custom_orders_table_data_sync_enabled', 'yes', 'yes'),
  ('woocommerce_ppcp-settings', 'a:2:{s:7:"enabled";s:3:"yes";s:5:"title";s:6:"PayPal";}', 'yes'),
  ('zzz_mystery_setting', 'x', 'no'),
  ('_transient_old_cache', 'x', 'no'),
  ('_transient_timeout_old_cache', '1000', 'no'),
  ('_transient_fresh_cache', 'y', 'no'),
  ('_transient_timeout_fresh_cache', '4102444800', 'no');

INSERT INTO wp_users (ID, user_login, user_pass, user_email, user_registered, display_name, user_activation_key) VALUES
  (1, 'admin', '$P$admin', 'admin@shop.test', '2025-01-01 00:00:00', 'Admin', ''),
  (12, 'jane', '$P$secret', 'jane@shop.test', '2026-01-02 10:00:00', 'Jane Doe', '1767348000:$P$Bresetkeyhash');
INSERT INTO wp_usermeta (user_id, meta_key, meta_value) VALUES
  (1, 'wp_capabilities', 'a:1:{s:13:"administrator";b:1;}'),
  (12, 'wp_capabilities', 'a:1:{s:8:"customer";b:1;}'),
  (12, 'first_name', 'Jane'),
  (12, 'last_name', 'Doe'),
  (12, 'billing_first_name', 'Jane'),
  (12, 'billing_city', 'Vienna'),
  (12, 'billing_country', 'AT'),
  (12, 'show_admin_bar_front', 'true'),
  (12, 'session_tokens', 'a:1:{s:4:"abcd";a:0:{}}');

INSERT INTO wp_terms (term_id, name, slug) VALUES (1, 'simple', 'simple'), (2, 'variable', 'variable'), (3, 'Mugs', 'mugs'), (4, 'Red', 'red'), (5, 'Blue', 'blue');
INSERT INTO wp_term_taxonomy (term_taxonomy_id, term_id, taxonomy) VALUES (1, 1, 'product_type'), (2, 2, 'product_type'), (3, 3, 'product_cat'), (4, 4, 'pa_color'), (5, 5, 'pa_color');
INSERT INTO wp_term_relationships (object_id, term_taxonomy_id, term_order) VALUES (123, 1, 0), (123, 3, 0), (200, 2, 0), (200, 4, 0), (200, 5, 1);

INSERT INTO wp_posts (ID, post_author, post_date_gmt, post_modified_gmt, post_title, post_name, post_status, post_type, post_parent) VALUES
  (123, 1, '2026-01-01 00:00:00', '2026-01-01 00:00:00', 'Mug', 'mug', 'publish', 'product', 0),
  (124, 1, '2026-01-01 00:00:00', '2026-01-01 00:00:00', 'mug.jpg', 'mug-jpg', 'inherit', 'attachment', 123),
  (200, 1, '2026-01-01 00:00:00', '2026-01-01 00:00:00', 'Shirt', 'shirt', 'publish', 'product', 0),
  (201, 1, '2026-01-01 00:00:00', '2026-01-01 00:00:00', 'Shirt - Red', '', 'publish', 'product_variation', 200),
  (202, 1, '2026-01-01 00:00:00', '2026-01-01 00:00:00', 'Shirt - Blue', '', 'publish', 'product_variation', 200),
  (300, 1, '2026-02-01 00:00:00', '2026-02-01 00:00:00', 'Hello world', 'hello-world', 'publish', 'post', 0),
  (4711, 0, UTC_TIMESTAMP() - INTERVAL 1 DAY, UTC_TIMESTAMP() - INTERVAL 1 DAY, '', '', 'wc-processing', 'shop_order', 0);
INSERT INTO wp_postmeta (post_id, meta_key, meta_value) VALUES
  (123, '_price', '24.95'), (123, '_sku', 'MUG-1'), (123, '_thumbnail_id', '124'), (123, '_stock_status', 'instock'),
  (124, '_wp_attached_file', '2026/01/mug.jpg'),
  (124, '_wp_attachment_metadata', 'a:2:{s:4:"file";s:15:"2026/01/mug.jpg";s:5:"sizes";a:1:{s:9:"thumbnail";a:1:{s:4:"file";s:15:"mug-150x150.jpg";}}}'),
  (200, '_product_attributes', 'a:1:{s:8:"pa_color";a:3:{s:4:"name";s:8:"pa_color";s:5:"value";s:0:"";s:11:"is_taxonomy";i:1;}}'),
  (201, 'attribute_pa_color', 'red'), (201, '_price', '20'), (201, '_sku', 'SHIRT-R'), (201, '_stock_status', 'instock'),
  (202, 'attribute_pa_color', 'blue'), (202, '_price', '22'), (202, '_sku', 'SHIRT-B'), (202, '_stock_status', 'outofstock');
INSERT INTO wp_wc_product_meta_lookup (product_id, sku, min_price, max_price, stock_quantity, stock_status, total_sales) VALUES (123, 'MUG-1', 24.95, 24.95, NULL, 'instock', 5), (200, '', 20, 22, NULL, 'instock', 1);
UPDATE wp_posts SET guid = 'https://shop.test/wp-content/uploads/2026/01/mug.jpg' WHERE ID = 124;

INSERT INTO wp_wc_orders (id, status, type, currency, total_amount, customer_id, billing_email, date_created_gmt, date_updated_gmt, parent_order_id, payment_method, payment_method_title, transaction_id, tax_amount) VALUES
  (4711, 'wc-processing', 'shop_order', 'EUR', 49.90, 12, 'jane@shop.test', UTC_TIMESTAMP() - INTERVAL 1 DAY, UTC_TIMESTAMP() - INTERVAL 1 DAY, 0, 'ppcp-gateway', 'PayPal', '61P3809224872131H', 7.97),
  (4712, 'wc-refunded', 'shop_order_refund', 'EUR', -10.00, 0, NULL, UTC_TIMESTAMP() - INTERVAL 2 HOUR, UTC_TIMESTAMP() - INTERVAL 2 HOUR, 4711, '', '', '', -1.60),
  (4713, 'wc-failed', 'shop_order', 'EUR', 18.90, 0, 'guest@shop.test', UTC_TIMESTAMP() - INTERVAL 3 DAY, UTC_TIMESTAMP() - INTERVAL 3 DAY, 0, 'ppcp-gateway', 'PayPal', '', 3.02),
  (4714, 'wc-active', 'shop_subscription', 'EUR', 9.90, 12, 'jane@shop.test', UTC_TIMESTAMP() - INTERVAL 1 DAY, UTC_TIMESTAMP() - INTERVAL 1 DAY, 4711, 'ppcp-gateway', 'PayPal', '', 1.58);
-- Both timestamps came from separate UTC_TIMESTAMP() calls; align them so only 4712 and 4713 (no posts copy) drift.
UPDATE wp_posts p JOIN wp_wc_orders o ON o.id = p.ID SET p.post_modified_gmt = o.date_updated_gmt;
INSERT INTO wp_wc_orders_meta (order_id, meta_key, meta_value) VALUES
  (4711, '_ppcp_paypal_order_id', '66M12696JX411763D'),
  (4711, '_ppcp_paypal_intent', 'CAPTURE'),
  (4711, '_ppcp_paypal_payment_mode', 'sandbox'),
  (4711, '_ppcp_paypal_fees', 'a:3:{s:12:"gross_amount";a:2:{s:13:"currency_code";s:3:"EUR";s:5:"value";s:5:"49.90";}s:10:"paypal_fee";a:2:{s:13:"currency_code";s:3:"EUR";s:5:"value";s:4:"2.04";}s:10:"net_amount";a:2:{s:13:"currency_code";s:3:"EUR";s:5:"value";s:5:"47.86";}}'),
  (4711, '_wc_order_attribution_source_type', 'typein'),
  (4711, '_wc_order_attribution_session_entry', 'https://shop.test/'),
  (4711, '_wc_order_attribution_session_start_time', DATE_FORMAT(UTC_TIMESTAMP() - INTERVAL 1 DAY - INTERVAL 10 MINUTE, '%Y-%m-%d %H:%i:%s')),
  (4711, '_wc_order_attribution_user_agent', 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/154.0.0.0 Safari/537.36');
INSERT INTO wp_wc_order_addresses (order_id, address_type, first_name, last_name, address_1, city, postcode, country, email) VALUES
  (4711, 'billing', 'Jane', 'Doe', 'Ring 1', 'Vienna', '1010', 'AT', 'jane@shop.test'),
  (4711, 'shipping', 'Jane', 'Doe', 'Marktstraße 1', 'Dornbirn', '6850', 'AT', NULL);
INSERT INTO wp_wc_order_operational_data (order_id, created_via, date_paid_gmt, order_key, shipping_tax_amount, shipping_total_amount, discount_tax_amount, discount_total_amount) VALUES (4711, 'checkout', UTC_TIMESTAMP() - INTERVAL 1 DAY + INTERVAL 1 MINUTE, 'wc_order_aBcDeFgH12345', 0.78, 4.90, 0.00, 0.00);
INSERT INTO wp_woocommerce_order_items (order_item_id, order_item_name, order_item_type, order_id) VALUES (1, 'Mug', 'line_item', 4711), (2, 'Flat rate', 'shipping', 4711), (3, 'Shirt - Red', 'line_item', 4713);
INSERT INTO wp_woocommerce_order_itemmeta (order_item_id, meta_key, meta_value) VALUES
  (1, '_product_id', '123'), (1, '_variation_id', '0'), (1, '_qty', '2'), (1, '_line_subtotal', '45.00'), (1, '_line_total', '45.00'), (1, '_line_tax', '0'),
  (2, 'cost', '4.90'), (2, 'total_tax', '0'), (2, 'method_id', 'flat_rate'),
  (3, '_product_id', '200'), (3, '_variation_id', '201'), (3, '_qty', '1'), (3, '_line_total', '18.90'), (3, 'pa_color', 'red');
INSERT INTO wp_wc_order_product_lookup (order_item_id, order_id, product_id, variation_id, customer_id, product_net_revenue, product_gross_revenue) VALUES (1, 4711, 123, 0, 1, 45, 45), (3, 4713, 200, 201, NULL, 18.9, 18.9);
INSERT INTO wp_comments (comment_post_ID, comment_author, comment_date_gmt, comment_content, comment_type, comment_author_email, comment_approved, user_id) VALUES
  (4711, 'WooCommerce', UTC_TIMESTAMP() - INTERVAL 1 DAY + INTERVAL 1 MINUTE, 'Payment captured', 'order_note', 'woocommerce@shop.test', '1', 0),
  (4711, 'Jane', UTC_TIMESTAMP(), 'Nice mug', 'review', 'jane@shop.test', '1', 12);
UPDATE wp_comments SET comment_date = comment_date_gmt + INTERVAL 2 HOUR;
INSERT INTO wp_commentmeta (comment_id, meta_key, meta_value) VALUES (1, 'is_customer_note', '0');

INSERT INTO wp_actionscheduler_groups (group_id, slug) VALUES (1, 'woocommerce-paypal-payments');
INSERT INTO wp_actionscheduler_actions (action_id, hook, status, scheduled_date_gmt, args, group_id, attempts, last_attempt_gmt) VALUES
  (1, 'ppcp_check_payment_status', 'failed', UTC_TIMESTAMP() - INTERVAL 1 DAY, '[4711]', 1, 1, UTC_TIMESTAMP() - INTERVAL 1 DAY),
  (2, 'woocommerce_cleanup_draft_orders', 'complete', UTC_TIMESTAMP() - INTERVAL 2 DAY, '[]', 0, 1, UTC_TIMESTAMP() - INTERVAL 2 DAY),
  (3, 'wc-admin_import_orders', 'pending', UTC_TIMESTAMP() + INTERVAL 1 HOUR, '[14711]', 0, 0, '0000-00-00 00:00:00'),
  (4, 'woocommerce_run_on_woocommerce_admin_updated', 'pending', UTC_TIMESTAMP() - INTERVAL 1 HOUR, '[]', 0, 0, '0000-00-00 00:00:00');
INSERT INTO wp_actionscheduler_logs (action_id, message, log_date_gmt, log_date_local) VALUES
  (1, 'action created', UTC_TIMESTAMP() - INTERVAL 1 DAY, UTC_TIMESTAMP() - INTERVAL 1 DAY + INTERVAL 2 HOUR),
  (1, 'action failed via WP Cron: PayPal API returned 500', UTC_TIMESTAMP() - INTERVAL 1 DAY, UTC_TIMESTAMP() - INTERVAL 1 DAY + INTERVAL 2 HOUR);

INSERT INTO wp_woocommerce_sessions (session_key, session_value, session_expiry) VALUES
  ('12', 'a:2:{s:4:"cart";s:41:"a:1:{s:3:"abc";a:1:{s:8:"quantity";i:2;}}";s:11:"cart_totals";s:30:"a:1:{s:5:"total";s:5:"49.90";}";}', UNIX_TIMESTAMP() + 86400),
  ('t_guestsession', 'a:0:{}', UNIX_TIMESTAMP() - 60);
INSERT INTO wp_woocommerce_log (timestamp, level, source, message, context) VALUES
  (UTC_TIMESTAMP() - INTERVAL 1 DAY, 100, 'woocommerce-paypal-payments', 'PATCH https://api-m.sandbox.paypal.com/v2/checkout/orders/66M12696JX411763D', NULL),
  (UTC_TIMESTAMP() - INTERVAL 1 DAY, 200, 'transactional-emails', 'Email "new_order" for order #4711 sent', NULL),
  (UTC_TIMESTAMP() - INTERVAL 1 DAY, 400, 'other', 'Unrelated error #14711', NULL),
  (UTC_TIMESTAMP() - INTERVAL 1 DAY, 200, 'other', 'Stock reduced for order #47110', NULL),
  (UTC_TIMESTAMP() - INTERVAL 1 DAY, 200, 'webhooks', 'Webhook delivered', 'a:1:{s:5:"order";i:4711;}');
INSERT INTO wp_woocommerce_payment_tokens (token_id, gateway_id, token, user_id, type, is_default) VALUES (7, 'ppcp-credit-card-gateway', 'tok_1', 12, 'CC', 1);
INSERT INTO wp_woocommerce_payment_tokenmeta (payment_token_id, meta_key, meta_value) VALUES (7, 'card_type', 'visa'), (7, 'last4', '2503'), (7, 'expiry_month', '01'), (7, 'expiry_year', '2029');

-- Writable copy for edit/undo tests, so they never race the read-only wp_test tests.
CREATE DATABASE wp_write CHARACTER SET utf8mb4;
CREATE TABLE wp_write.wp_options LIKE wp_test.wp_options;
INSERT INTO wp_write.wp_options SELECT * FROM wp_test.wp_options;
CREATE TABLE wp_write.wp_users LIKE wp_test.wp_users;
INSERT INTO wp_write.wp_users SELECT * FROM wp_test.wp_users;
CREATE TABLE wp_write.wp_usermeta LIKE wp_test.wp_usermeta;
INSERT INTO wp_write.wp_usermeta SELECT * FROM wp_test.wp_usermeta;
CREATE TABLE wp_write.wp_posts LIKE wp_test.wp_posts;
INSERT INTO wp_write.wp_posts SELECT * FROM wp_test.wp_posts;
CREATE TABLE wp_write.wp_postmeta LIKE wp_test.wp_postmeta;
INSERT INTO wp_write.wp_postmeta SELECT * FROM wp_test.wp_postmeta;
CREATE TABLE wp_write.wp_comments LIKE wp_test.wp_comments;
INSERT INTO wp_write.wp_comments SELECT * FROM wp_test.wp_comments;
CREATE TABLE wp_write.wp_commentmeta LIKE wp_test.wp_commentmeta;
INSERT INTO wp_write.wp_commentmeta SELECT * FROM wp_test.wp_commentmeta;
CREATE TABLE wp_write.wp_terms LIKE wp_test.wp_terms;
INSERT INTO wp_write.wp_terms SELECT * FROM wp_test.wp_terms;
CREATE TABLE wp_write.wp_term_taxonomy LIKE wp_test.wp_term_taxonomy;
INSERT INTO wp_write.wp_term_taxonomy SELECT * FROM wp_test.wp_term_taxonomy;
CREATE TABLE wp_write.wp_term_relationships LIKE wp_test.wp_term_relationships;
INSERT INTO wp_write.wp_term_relationships SELECT * FROM wp_test.wp_term_relationships;
CREATE TABLE wp_write.wp_wc_orders LIKE wp_test.wp_wc_orders;
INSERT INTO wp_write.wp_wc_orders SELECT * FROM wp_test.wp_wc_orders;
CREATE TABLE wp_write.wp_wc_orders_meta LIKE wp_test.wp_wc_orders_meta;
INSERT INTO wp_write.wp_wc_orders_meta SELECT * FROM wp_test.wp_wc_orders_meta;
CREATE TABLE wp_write.wp_wc_order_addresses LIKE wp_test.wp_wc_order_addresses;
INSERT INTO wp_write.wp_wc_order_addresses SELECT * FROM wp_test.wp_wc_order_addresses;
CREATE TABLE wp_write.wp_wc_order_operational_data LIKE wp_test.wp_wc_order_operational_data;
INSERT INTO wp_write.wp_wc_order_operational_data SELECT * FROM wp_test.wp_wc_order_operational_data;
CREATE TABLE wp_write.wp_woocommerce_order_items LIKE wp_test.wp_woocommerce_order_items;
INSERT INTO wp_write.wp_woocommerce_order_items SELECT * FROM wp_test.wp_woocommerce_order_items;
CREATE TABLE wp_write.wp_woocommerce_order_itemmeta LIKE wp_test.wp_woocommerce_order_itemmeta;
INSERT INTO wp_write.wp_woocommerce_order_itemmeta SELECT * FROM wp_test.wp_woocommerce_order_itemmeta;
CREATE TABLE wp_write.wp_wc_product_meta_lookup LIKE wp_test.wp_wc_product_meta_lookup;
INSERT INTO wp_write.wp_wc_product_meta_lookup (product_id, sku, min_price, max_price, stock_quantity, stock_status, total_sales) SELECT product_id, sku, min_price, max_price, stock_quantity, stock_status, total_sales FROM wp_test.wp_wc_product_meta_lookup;
CREATE TABLE wp_write.wp_wc_order_product_lookup LIKE wp_test.wp_wc_order_product_lookup;
INSERT INTO wp_write.wp_wc_order_product_lookup SELECT * FROM wp_test.wp_wc_order_product_lookup;
CREATE TABLE wp_write.wp_actionscheduler_actions LIKE wp_test.wp_actionscheduler_actions;
INSERT INTO wp_write.wp_actionscheduler_actions SELECT * FROM wp_test.wp_actionscheduler_actions;
CREATE TABLE wp_write.wp_actionscheduler_groups LIKE wp_test.wp_actionscheduler_groups;
INSERT INTO wp_write.wp_actionscheduler_groups SELECT * FROM wp_test.wp_actionscheduler_groups;
CREATE TABLE wp_write.wp_actionscheduler_logs LIKE wp_test.wp_actionscheduler_logs;
INSERT INTO wp_write.wp_actionscheduler_logs SELECT * FROM wp_test.wp_actionscheduler_logs;
CREATE TABLE wp_write.wp_woocommerce_sessions LIKE wp_test.wp_woocommerce_sessions;
INSERT INTO wp_write.wp_woocommerce_sessions SELECT * FROM wp_test.wp_woocommerce_sessions;
CREATE TABLE wp_write.wp_woocommerce_log LIKE wp_test.wp_woocommerce_log;
INSERT INTO wp_write.wp_woocommerce_log SELECT * FROM wp_test.wp_woocommerce_log;
CREATE TABLE wp_write.wp_woocommerce_payment_tokens LIKE wp_test.wp_woocommerce_payment_tokens;
INSERT INTO wp_write.wp_woocommerce_payment_tokens SELECT * FROM wp_test.wp_woocommerce_payment_tokens;
CREATE TABLE wp_write.wp_woocommerce_payment_tokenmeta LIKE wp_test.wp_woocommerce_payment_tokenmeta;
INSERT INTO wp_write.wp_woocommerce_payment_tokenmeta SELECT * FROM wp_test.wp_woocommerce_payment_tokenmeta;

-- Rows each owned by one write test (ids from 9100 up), so no two tests share a row.
-- Orders: 9101 status change (HPOS row plus posts copy), 9102 new meta, 9103 meta delete, 9105 rollback,
-- 9401 and 9403 comparison, 9402 snapshot, 9602 NULL address email.
INSERT INTO wp_write.wp_wc_orders (id, status, type, currency, total_amount, customer_id, billing_email, date_created_gmt, date_updated_gmt, parent_order_id, payment_method, payment_method_title, transaction_id) VALUES
  (9101, 'wc-processing', 'shop_order', 'EUR', 10.00, 0, 'status@shop.test', '2026-01-01 10:00:00', '2026-01-01 10:00:00', 0, 'bacs', 'Bank transfer', ''),
  (9102, 'wc-processing', 'shop_order', 'EUR', 10.00, 0, 'setmeta@shop.test', '2026-01-01 10:00:00', '2026-01-01 10:00:00', 0, 'bacs', 'Bank transfer', ''),
  (9103, 'wc-processing', 'shop_order', 'EUR', 10.00, 0, 'deletemeta@shop.test', '2026-01-01 10:00:00', '2026-01-01 10:00:00', 0, 'bacs', 'Bank transfer', ''),
  (9105, 'wc-failed', 'shop_order', 'EUR', 10.00, 0, 'rollback@shop.test', '2026-01-01 10:00:00', '2026-01-01 10:00:00', 0, 'bacs', 'Bank transfer', ''),
  (9401, 'wc-processing', 'shop_order', 'EUR', 49.90, 0, 'compare-a@shop.test', '2026-01-01 10:00:00', '2026-01-01 10:00:00', 0, 'bacs', 'Bank transfer', ''),
  (9402, 'wc-failed', 'shop_order', 'EUR', 18.90, 0, 'snapshot@shop.test', '2026-01-01 10:00:00', '2026-01-01 10:00:00', 0, 'bacs', 'Bank transfer', ''),
  (9403, 'wc-failed', 'shop_order', 'EUR', 18.90, 0, 'compare-b@shop.test', '2026-01-01 10:00:00', '2026-01-01 10:00:00', 0, 'bacs', 'Bank transfer', ''),
  (9602, 'wc-processing', 'shop_order', 'EUR', 10.00, 0, 'address@shop.test', '2026-01-01 10:00:00', '2026-01-01 10:00:00', 0, 'bacs', 'Bank transfer', '');
INSERT INTO wp_write.wp_posts (ID, post_author, post_date_gmt, post_modified_gmt, post_modified, post_title, post_name, post_status, post_type, post_parent) VALUES
  (9101, 0, '2026-01-01 10:00:00', '2026-01-01 10:00:00', '2026-01-01 11:00:00', '', '', 'wc-processing', 'shop_order', 0),
  (9601, 1, '2026-01-01 10:00:00', '2026-01-01 10:00:00', '2026-01-01 11:00:00', 'Empty slug', '', 'publish', 'post', 0),
  (9701, 1, '2026-01-01 10:00:00', '2026-01-01 10:00:00', '2026-01-01 11:00:00', 'Long meta', 'long-meta', 'publish', 'post', 0);
INSERT INTO wp_write.wp_wc_orders_meta (order_id, meta_key, meta_value) VALUES
  (9103, '_keep', 'k'),
  (9103, '_to_delete', 'bye'),
  (9401, '_ppcp_paypal_fees', 'a:3:{s:12:"gross_amount";a:2:{s:13:"currency_code";s:3:"EUR";s:5:"value";s:5:"49.90";}s:10:"paypal_fee";a:2:{s:13:"currency_code";s:3:"EUR";s:5:"value";s:4:"2.04";}s:10:"net_amount";a:2:{s:13:"currency_code";s:3:"EUR";s:5:"value";s:5:"47.86";}}');
INSERT INTO wp_write.wp_wc_order_addresses (id, order_id, address_type, first_name, last_name, address_1, city, postcode, country, email) VALUES
  (9602, 9602, 'shipping', 'Ship', 'Target', 'Marktstraße 1', 'Dornbirn', '6850', 'AT', NULL);
INSERT INTO wp_write.wp_woocommerce_order_items (order_item_id, order_item_name, order_item_type, order_id) VALUES (9402, 'Snapshot item', 'line_item', 9402);
INSERT INTO wp_write.wp_woocommerce_order_itemmeta (order_item_id, meta_key, meta_value) VALUES (9402, '_qty', '3');
INSERT INTO wp_write.wp_options (option_name, option_value, autoload) VALUES
  ('zz_serialized_edit', 'a:2:{s:7:"enabled";s:3:"yes";s:5:"title";s:6:"PayPal";}', 'yes'),
  ('zz_autoload_probe', 'x', 'yes'),
  ('zz_rename_me', 'x', 'no'),
  ('zz_snapshot_probe', 'before', 'no');
INSERT INTO wp_write.wp_actionscheduler_actions (action_id, hook, status, scheduled_date_gmt, args, group_id, attempts) VALUES
  (9201, 'zz_retry_hook', 'failed', '2026-01-01 00:00:00', '[]', 0, 1),
  (9202, 'zz_cancel_hook', 'pending', '2030-01-01 00:00:00', '[]', 0, 0);
INSERT INTO wp_write.wp_postmeta (meta_id, post_id, meta_key, meta_value) VALUES
  (9301, 123, 'zz_null_meta', NULL),
  (9302, 123, 'zz_duplicate_me', 'v'),
  (9303, 123, 'zz_sql_mode', 'before');
-- Post 9701 has 5,000 meta keys (meta_id 100001 to 105000) for the long document group and Rows grid.
-- Digits cross-joined instead of a recursive CTE: MySQL stops recursion at 1,000 rows by default.
INSERT INTO wp_write.wp_postmeta (meta_id, post_id, meta_key, meta_value)
SELECT 100000 + n, 9701, CONCAT('zz_long_', LPAD(n, 4, '0')), CONCAT('value ', n)
FROM (
  SELECT 1 + ones.d + 10 * tens.d + 100 * hundreds.d + 1000 * thousands.d AS n
  FROM (SELECT 0 AS d UNION ALL SELECT 1 UNION ALL SELECT 2 UNION ALL SELECT 3 UNION ALL SELECT 4 UNION ALL SELECT 5 UNION ALL SELECT 6 UNION ALL SELECT 7 UNION ALL SELECT 8 UNION ALL SELECT 9) AS ones
  CROSS JOIN (SELECT 0 AS d UNION ALL SELECT 1 UNION ALL SELECT 2 UNION ALL SELECT 3 UNION ALL SELECT 4 UNION ALL SELECT 5 UNION ALL SELECT 6 UNION ALL SELECT 7 UNION ALL SELECT 8 UNION ALL SELECT 9) AS tens
  CROSS JOIN (SELECT 0 AS d UNION ALL SELECT 1 UNION ALL SELECT 2 UNION ALL SELECT 3 UNION ALL SELECT 4 UNION ALL SELECT 5 UNION ALL SELECT 6 UNION ALL SELECT 7 UNION ALL SELECT 8 UNION ALL SELECT 9) AS hundreds
  CROSS JOIN (SELECT 0 AS d UNION ALL SELECT 1 UNION ALL SELECT 2 UNION ALL SELECT 3 UNION ALL SELECT 4) AS thousands
) AS seq;
INSERT INTO wp_write.wp_usermeta (user_id, meta_key, meta_value) VALUES
  (9501, 'dup_key', 'a'),
  (9501, 'dup_key', 'b');
INSERT INTO wp_write.wp_woocommerce_log (timestamp, level, source, message, context) VALUES
  (UTC_TIMESTAMP() - INTERVAL 400 DAY, 200, 'zz-retention', 'old entry one', NULL),
  (UTC_TIMESTAMP() - INTERVAL 400 DAY, 200, 'zz-retention', 'old entry two', NULL),
  (UTC_TIMESTAMP() - INTERVAL 1 DAY, 200, 'zz-retention', 'recent entry', NULL);

-- Legacy WooCommerce install (orders in wp_posts, HPOS off, no wc_orders tables).
CREATE DATABASE wp_legacy CHARACTER SET utf8mb4;
USE wp_legacy;
CREATE TABLE wp_options LIKE wp_test.wp_options;
CREATE TABLE wp_users LIKE wp_test.wp_users;
CREATE TABLE wp_usermeta LIKE wp_test.wp_usermeta;
CREATE TABLE wp_posts LIKE wp_test.wp_posts;
CREATE TABLE wp_postmeta LIKE wp_test.wp_postmeta;
CREATE TABLE wp_comments LIKE wp_test.wp_comments;
CREATE TABLE wp_commentmeta LIKE wp_test.wp_commentmeta;
CREATE TABLE wp_terms LIKE wp_test.wp_terms;
CREATE TABLE wp_term_taxonomy LIKE wp_test.wp_term_taxonomy;
CREATE TABLE wp_term_relationships LIKE wp_test.wp_term_relationships;
CREATE TABLE wp_woocommerce_order_items LIKE wp_test.wp_woocommerce_order_items;
CREATE TABLE wp_woocommerce_order_itemmeta LIKE wp_test.wp_woocommerce_order_itemmeta;
CREATE TABLE wp_actionscheduler_actions LIKE wp_test.wp_actionscheduler_actions;
CREATE TABLE wp_actionscheduler_groups LIKE wp_test.wp_actionscheduler_groups;
CREATE TABLE wp_actionscheduler_logs LIKE wp_test.wp_actionscheduler_logs;
CREATE TABLE wp_woocommerce_log LIKE wp_test.wp_woocommerce_log;
CREATE TABLE wp_woocommerce_payment_tokens LIKE wp_test.wp_woocommerce_payment_tokens;
CREATE TABLE wp_woocommerce_payment_tokenmeta LIKE wp_test.wp_woocommerce_payment_tokenmeta;

INSERT INTO wp_options (option_name, option_value, autoload) VALUES
  ('siteurl', 'https://legacy.test', 'yes'),
  ('blogname', 'Legacy Shop', 'yes'),
  ('db_version', '57155', 'yes'),
  ('woocommerce_version', '8.2.1', 'yes'),
  ('woocommerce_currency', 'USD', 'yes'),
  ('active_plugins', 'a:1:{i:0;s:27:"woocommerce/woocommerce.php";}', 'yes'),
  ('woocommerce_custom_orders_table_enabled', 'no', 'yes'),
  ('woocommerce_custom_orders_table_data_sync_enabled', 'no', 'yes');

INSERT INTO wp_users (ID, user_login, user_pass, user_email, user_registered, display_name) VALUES
  (1, 'admin', '$P$admin', 'admin@legacy.test', '2024-01-01 00:00:00', 'Admin'),
  (7, 'sam', '$P$secret', 'sam@legacy.test', '2025-03-04 09:00:00', 'Sam Smith');
INSERT INTO wp_usermeta (user_id, meta_key, meta_value) VALUES
  (1, 'wp_capabilities', 'a:1:{s:13:"administrator";b:1;}'),
  (7, 'wp_capabilities', 'a:1:{s:8:"customer";b:1;}'),
  (7, 'billing_first_name', 'Sam'),
  (7, 'billing_email', 'sam@legacy.test');

INSERT INTO wp_posts (ID, post_author, post_date_gmt, post_modified_gmt, post_title, post_name, post_status, post_type, post_parent) VALUES
  (10, 1, '2025-01-01 00:00:00', '2025-01-01 00:00:00', 'Poster', 'poster', 'publish', 'product', 0),
  (501, 1, UTC_TIMESTAMP() - INTERVAL 2 DAY, UTC_TIMESTAMP() - INTERVAL 2 DAY, 'Order &ndash; 501', 'order-501', 'wc-completed', 'shop_order', 0),
  (502, 1, UTC_TIMESTAMP() - INTERVAL 1 DAY, UTC_TIMESTAMP() - INTERVAL 1 DAY, 'Order &ndash; 502', 'order-502', 'wc-processing', 'shop_order', 0),
  (503, 1, UTC_TIMESTAMP() - INTERVAL 3 HOUR, UTC_TIMESTAMP() - INTERVAL 3 HOUR, 'Order &ndash; 503', 'order-503', 'wc-on-hold', 'shop_order', 0),
  (504, 1, UTC_TIMESTAMP() - INTERVAL 1 HOUR, UTC_TIMESTAMP() - INTERVAL 1 HOUR, 'Refund &ndash; 504', 'refund-504', 'wc-completed', 'shop_order_refund', 501);
INSERT INTO wp_postmeta (post_id, meta_key, meta_value) VALUES
  (10, '_price', '15.00'), (10, '_sku', 'POSTER-1'),
  (501, '_customer_user', '7'), (501, '_order_total', '30.00'), (501, '_order_currency', 'USD'),
  (501, '_payment_method', 'ppcp-gateway'), (501, '_payment_method_title', 'PayPal'), (501, '_transaction_id', '8AB12345CD678901E'),
  (501, '_billing_first_name', 'Sam'), (501, '_billing_last_name', 'Smith'), (501, '_billing_email', 'sam@legacy.test'), (501, '_billing_city', 'Austin'), (501, '_billing_country', 'US'),
  (502, '_customer_user', '7'), (502, '_order_total', '15.00'), (502, '_order_currency', 'USD'),
  (502, '_payment_method', 'ppcp-gateway'), (502, '_payment_method_title', 'PayPal'),
  (502, '_billing_first_name', 'Sam'), (502, '_billing_last_name', 'Smith'), (502, '_billing_email', 'sam@legacy.test'),
  (503, '_customer_user', '0'), (503, '_order_total', '15.00'), (503, '_order_currency', 'USD'),
  (503, '_payment_method', 'bacs'), (503, '_payment_method_title', 'Direct bank transfer'),
  (503, '_billing_first_name', 'Gail'), (503, '_billing_last_name', 'Guest'), (503, '_billing_email', 'guest@legacy.test'),
  (504, '_order_total', '-5.00'), (504, '_order_currency', 'USD'), (504, '_refund_amount', '5.00');
INSERT INTO wp_woocommerce_order_items (order_item_id, order_item_name, order_item_type, order_id) VALUES
  (1, 'Poster', 'line_item', 501), (2, 'Poster', 'line_item', 502), (3, 'Poster', 'line_item', 503);
INSERT INTO wp_woocommerce_order_itemmeta (order_item_id, meta_key, meta_value) VALUES
  (1, '_product_id', '10'), (1, '_qty', '2'), (1, '_line_total', '30.00'),
  (2, '_product_id', '10'), (2, '_qty', '1'), (2, '_line_total', '15.00'),
  (3, '_product_id', '10'), (3, '_qty', '1'), (3, '_line_total', '15.00');
INSERT INTO wp_comments (comment_post_ID, comment_author, comment_date_gmt, comment_content, comment_type, comment_author_email) VALUES
  (501, 'WooCommerce', UTC_TIMESTAMP() - INTERVAL 2 DAY, 'Order status changed from Processing to Completed.', 'order_note', 'woocommerce@legacy.test'),
  (501, 'WooCommerce', UTC_TIMESTAMP() - INTERVAL 1 HOUR, 'Refunded $5.00', 'order_note', 'woocommerce@legacy.test');
UPDATE wp_comments SET comment_date = comment_date_gmt;
INSERT INTO wp_woocommerce_log (timestamp, level, source, message, context) VALUES
  (UTC_TIMESTAMP() - INTERVAL 2 DAY, 200, 'transactional-emails', 'Email "customer_completed_order" for order #501 sent', NULL);

-- Custom table prefix (xy_) for prefix detection.
CREATE DATABASE wp_custom CHARACTER SET utf8mb4;
USE wp_custom;
CREATE TABLE xy_options LIKE wp_test.wp_options;
CREATE TABLE xy_users LIKE wp_test.wp_users;
CREATE TABLE xy_usermeta LIKE wp_test.wp_usermeta;
CREATE TABLE xy_posts LIKE wp_test.wp_posts;
CREATE TABLE xy_postmeta LIKE wp_test.wp_postmeta;

INSERT INTO xy_options (option_name, option_value, autoload) VALUES
  ('siteurl', 'https://custom.test', 'yes'),
  ('blogname', 'Custom Prefix Site', 'yes'),
  ('db_version', '58975', 'yes'),
  ('active_plugins', 'a:0:{}', 'yes');
INSERT INTO xy_users (ID, user_login, user_pass, user_email, user_registered, display_name) VALUES
  (1, 'admin', '$P$admin', 'admin@custom.test', '2025-01-01 00:00:00', 'Admin');
INSERT INTO xy_usermeta (user_id, meta_key, meta_value) VALUES
  (1, 'xy_capabilities', 'a:1:{s:13:"administrator";b:1;}');
INSERT INTO xy_posts (ID, post_author, post_date_gmt, post_modified_gmt, post_title, post_name, post_status, post_type, post_parent) VALUES
  (1, 1, '2025-01-01 00:00:00', '2025-01-01 00:00:00', 'Hello world', 'hello-world', 'publish', 'post', 0);

-- Multisite network: main site (wp_) and sub-site 2 (wp_2_, no wp_2_users: users are network-wide).
CREATE DATABASE wp_multi CHARACTER SET utf8mb4;
USE wp_multi;
CREATE TABLE wp_options LIKE wp_test.wp_options;
CREATE TABLE wp_users LIKE wp_test.wp_users;
CREATE TABLE wp_usermeta LIKE wp_test.wp_usermeta;
CREATE TABLE wp_posts LIKE wp_test.wp_posts;
CREATE TABLE wp_postmeta LIKE wp_test.wp_postmeta;
CREATE TABLE wp_2_options LIKE wp_test.wp_options;
CREATE TABLE wp_2_posts LIKE wp_test.wp_posts;
CREATE TABLE wp_2_postmeta LIKE wp_test.wp_postmeta;
CREATE TABLE wp_blogs (blog_id bigint(20) NOT NULL AUTO_INCREMENT, site_id bigint(20) NOT NULL DEFAULT 0, domain varchar(200) NOT NULL DEFAULT '', path varchar(100) NOT NULL DEFAULT '', registered datetime NOT NULL DEFAULT '0000-00-00 00:00:00', last_updated datetime NOT NULL DEFAULT '0000-00-00 00:00:00', public tinyint(2) NOT NULL DEFAULT 1, archived tinyint(2) NOT NULL DEFAULT 0, mature tinyint(2) NOT NULL DEFAULT 0, spam tinyint(2) NOT NULL DEFAULT 0, deleted tinyint(2) NOT NULL DEFAULT 0, lang_id int(11) NOT NULL DEFAULT 0, PRIMARY KEY (blog_id));
CREATE TABLE wp_sitemeta (meta_id bigint(20) NOT NULL AUTO_INCREMENT, site_id bigint(20) NOT NULL DEFAULT 0, meta_key varchar(255) DEFAULT NULL, meta_value longtext, PRIMARY KEY (meta_id));
CREATE TABLE wp_site (id bigint(20) unsigned NOT NULL AUTO_INCREMENT, domain varchar(200) NOT NULL DEFAULT '', path varchar(100) NOT NULL DEFAULT '', PRIMARY KEY (id));

INSERT INTO wp_site (id, domain, path) VALUES (1, 'network.test', '/');
INSERT INTO wp_blogs (blog_id, site_id, domain, path) VALUES
  (1, 1, 'network.test', '/'),
  (2, 1, 'shop.network.test', '/');
INSERT INTO wp_sitemeta (site_id, meta_key, meta_value) VALUES
  (1, 'site_name', 'Test Network'),
  (1, 'active_sitewide_plugins', 'a:1:{s:27:"woocommerce/woocommerce.php";i:1700000000;}');
INSERT INTO wp_options (option_name, option_value, autoload) VALUES
  ('siteurl', 'https://network.test', 'yes'),
  ('blogname', 'Network Main', 'yes'),
  ('db_version', '58975', 'yes'),
  ('active_plugins', 'a:0:{}', 'yes');
INSERT INTO wp_2_options (option_name, option_value, autoload) VALUES
  ('siteurl', 'https://shop.network.test', 'yes'),
  ('blogname', 'Network Shop', 'yes'),
  ('db_version', '58975', 'yes');
INSERT INTO wp_users (ID, user_login, user_pass, user_email, user_registered, display_name) VALUES
  (1, 'admin', '$P$admin', 'admin@network.test', '2025-01-01 00:00:00', 'Admin');
INSERT INTO wp_posts (ID, post_author, post_date_gmt, post_modified_gmt, post_title, post_name, post_status, post_type, post_parent) VALUES
  (1, 1, '2025-01-01 00:00:00', '2025-01-01 00:00:00', 'Main hello', 'main-hello', 'publish', 'post', 0);
INSERT INTO wp_2_posts (ID, post_author, post_date_gmt, post_modified_gmt, post_title, post_name, post_status, post_type, post_parent) VALUES
  (1, 1, '2025-01-01 00:00:00', '2025-01-01 00:00:00', 'Shop hello', 'shop-hello', 'publish', 'post', 0);
