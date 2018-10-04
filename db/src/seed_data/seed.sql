INSERT INTO public.users (id, first_name, last_name, email, phone, hashed_pw, password_modified_at, created_at, last_used, active, role, password_reset_token, password_reset_requested_at) VALUES ('4d72ddc1-fabf-4ac3-af9d-d6267ff57924', 'Mike', 'Berry', 'mike@tari.com', '555', '$argon2i$m=4096,t=3,p=1$akJ0Y1lJU0hWSVR2dWdyT3NoWWtBYlVjQ1doTlFHN3A$cfrrhqCbmmEtOC5/kOi7DC6LKrfTJ1YYeKM/k/CezWo', '2018-08-22 10:01:59.338182', '2018-08-22 10:01:59.338182', null, true, '{Guest,Admin}', null, null);
INSERT INTO public.users (id, first_name, last_name, email, phone, hashed_pw, password_modified_at, created_at, last_used, active, role, password_reset_token, password_reset_requested_at) VALUES ('7ce5fbe5-b0ba-486a-b6a1-6cf478ffdd8f', 'superer', 'user', 'super@test.com', '555', '$argon2i$m=4096,t=3,p=1$R2tPUkFXVnhlVWhUeVFaZzJyNXYwTldJY1paVURIS1E$irQHarAoMVVqjMbE2aEik7RNoIIH03xQVqsIsVR5tcU', '2018-08-24 09:36:26.070084', '2018-08-24 09:36:26.070084', null, true, '{User,Admin}', null, null);
INSERT INTO public.users (id, first_name, last_name, email, phone, hashed_pw, password_modified_at, created_at, last_used, active, role, password_reset_token, password_reset_requested_at) VALUES ('6f48256f-d18e-403e-b844-f001075eaef7', 'super', 'user', 'superuser@test.com', '555', '$argon2i$m=4096,t=3,p=1$YXZtcDNVZ2NCem5uZ2daeFFwbVJLMkZyNXlmY3Z3Z1E$LDJi8uZd3lO7IsZ7G3UmWcTu8ijpuYdiDP+IzftIaW8', '2018-08-24 09:36:26.070084', '2018-08-24 09:36:26.070084', null, true, '{User,Admin}', null, null);

INSERT INTO public.wallets (id, user_id, organization_id, name, created_at, updated_at) VALUES ('f65acaac-7331-4bec-bf72-7657a896f49d', '4d72ddc1-fabf-4ac3-af9d-d6267ff57924', null, 'Default', '2018-10-03 15:11:26.475501', '2018-10-03 15:11:26.475501');
INSERT INTO public.wallets (id, user_id, organization_id, name, created_at, updated_at) VALUES ('35cd481a-76b3-483e-8598-6ddfb13e67ec', '7ce5fbe5-b0ba-486a-b6a1-6cf478ffdd8f', null, 'Default', '2018-10-03 15:11:46.485989', '2018-10-03 15:11:46.485989');

INSERT INTO public.fee_schedules (id, name, version, created_at, updated_at) VALUES ('7730da06-b35c-4e7b-8608-baec34041fc2', 'Zero', 1, '2018-09-27 12:15:47.841257', '2018-09-27 12:15:47.841257');
INSERT INTO public.fee_schedule_ranges(fee_schedule_id, min_price, fee_in_cents) VALUES ('7730da06-b35c-4e7b-8608-baec34041fc2',0,0);

INSERT INTO public.organizations (id, owner_user_id, name, address, city, state, country, postal_code, phone, fee_schedule_id) VALUES ('ac1e48f2-6765-4a18-b43c-d3c9836bc4c3', '4d72ddc1-fabf-4ac3-af9d-d6267ff57924', 'Jazzy', null, null, null, null, null, null, '7730da06-b35c-4e7b-8608-baec34041fc2');

INSERT INTO public.venues (id, name, address, city, state, country, postal_code, phone) VALUES ('bd24baee-c074-46a7-b5c9-8bdfefb10ef5', 'Test venue 2', null, null, null, null, null, null);
INSERT INTO public.venues (id, name, address, city, state, country, postal_code, phone) VALUES ('0eb7fa9d-6a80-4c21-ac5c-d0682ab7dae6', 'Test venue 1', null, null, null, null, null, null);

INSERT INTO public.events (id, name, organization_id, venue_id, created_at, event_start, door_time, status, publish_date, promo_image_url, additional_info, age_limit, cancelled_at) VALUES ('c2cbae75-e2f9-442a-9e0d-b2a288aca009', 'Event1', 'ac1e48f2-6765-4a18-b43c-d3c9836bc4c3', '0eb7fa9d-6a80-4c21-ac5c-d0682ab7dae6', '2018-08-24 10:09:47.391560', '2018-11-12 12:00:00.000000', '2018-11-12 12:00:00.000000', 'Draft', '2018-11-12 12:00:00.000000', null, null, null, null);
INSERT INTO public.events (id, name, organization_id, venue_id, created_at, event_start, door_time, status, publish_date, promo_image_url, additional_info, age_limit, cancelled_at) VALUES ('199d8f0b-3f38-43aa-88b0-57c6ba4c0903', 'Event2', 'ac1e48f2-6765-4a18-b43c-d3c9836bc4c3', '0eb7fa9d-6a80-4c21-ac5c-d0682ab7dae6', '2018-08-24 10:10:05.423149', '2018-11-12 12:00:00.000000', '2018-11-12 12:00:00.000000', 'Draft', '2018-11-12 12:00:00.000000', null, null, null, null);
INSERT INTO public.events (id, name, organization_id, venue_id, created_at, event_start, door_time, status, publish_date, promo_image_url, additional_info, age_limit, cancelled_at) VALUES ('e8d3883f-596c-47ff-832c-e0882d44d22b', 'Event3', 'ac1e48f2-6765-4a18-b43c-d3c9836bc4c3', 'bd24baee-c074-46a7-b5c9-8bdfefb10ef5', '2018-08-24 10:10:25.714517', '2018-11-12 12:00:00.000000', '2018-11-12 12:00:00.000000', 'Draft', '2018-11-12 12:00:00.000000', null, null, null, null);

INSERT INTO public.artists (id, name, bio, website_url, youtube_video_urls, facebook_username, instagram_username, snapchat_username, soundcloud_username, bandcamp_username) VALUES ('f0784ac8-b026-4c67-82bd-50b20f077f27', 'Artist1', 'Some stuff', null, '{http://test.com,http://test2.com}', null, null, null, null, null);
INSERT INTO public.artists (id, name, bio, website_url, youtube_video_urls, facebook_username, instagram_username, snapchat_username, soundcloud_username, bandcamp_username) VALUES ('d4c4fe89-21d5-4f6a-b6fe-95ff2fdee87f', 'Artist2', 'Some stuff', null, '{http://test.com,http://test2.com}', null, null, null, null, null);
