CREATE TABLE "users" (
  "id" uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  "email" varchar(200) NOT NULL,
  "tenant_id" uuid NOT NULL,
  "active" boolean DEFAULT true NOT NULL,
  CONSTRAINT "users_email_readable" CHECK (length("email") > 3)
);
