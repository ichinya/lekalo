CREATE TABLE "users" (
  "id" uuid PRIMARY KEY,
  "email" text NOT NULL,
  "tenant_id" uuid NOT NULL
);
CREATE TABLE "posts" (
  "id" serial PRIMARY KEY,
  "author_id" integer NOT NULL,
  "title" text
);
