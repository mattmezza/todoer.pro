-- Freeze the published conversion deductions; never recompute old purchases.
ALTER TABLE billing_purchases ADD COLUMN gross_micro INTEGER NOT NULL DEFAULT 0 CHECK(gross_micro>=0);
ALTER TABLE billing_purchases ADD COLUMN payment_fee_allowance_micro INTEGER NOT NULL DEFAULT 0 CHECK(payment_fee_allowance_micro>=0);
ALTER TABLE billing_purchases ADD COLUMN forex_fee_micro INTEGER NOT NULL DEFAULT 0 CHECK(forex_fee_micro>=0);
UPDATE billing_purchases SET gross_micro=credit_micro;
ALTER TABLE billing_auto_topup ADD COLUMN fee_bps INTEGER NOT NULL DEFAULT 0;
ALTER TABLE billing_auto_topup ADD COLUMN fee_fixed_micro INTEGER NOT NULL DEFAULT 0;
ALTER TABLE billing_adjustments ADD COLUMN gross_refund_micro INTEGER NOT NULL DEFAULT 0;
ALTER TABLE billing_adjustments ADD COLUMN conversion_reversal_micro INTEGER NOT NULL DEFAULT 0;
UPDATE billing_adjustments SET gross_refund_micro=amount_micro+debt_micro;
