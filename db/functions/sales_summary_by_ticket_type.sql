DROP FUNCTION IF EXISTS sales_summary_by_ticket_type(p_transaction_start_date TIMESTAMP, p_transaction_end_date TIMESTAMP, p_event_start_date TIMESTAMP, p_event_end_date TIMESTAMP, p_organization_id UUID, p_page BIGINT, p_limit BIGINT);
CREATE OR REPLACE FUNCTION sales_summary_by_ticket_type(p_transaction_start_date TIMESTAMP, p_transaction_end_date TIMESTAMP, p_event_start_date TIMESTAMP, p_event_end_date TIMESTAMP, p_organization_id UUID, p_page BIGINT, p_limit BIGINT) RETURNS
TABLE
        (
            total                             BIGINT,
            event_name                        TEXT,
            event_date                        TIMESTAMP,
            ticket_name                       TEXT,
            face_value_in_cents               BIGINT,
            online_sale_count                 BIGINT,
            total_online_client_fees_in_cents BIGINT,
            box_office_sale_count             BIGINT,
            comp_sale_count                   BIGINT
        )
AS $$
BEGIN

DROP TABLE IF EXISTS order_item_ids;
CREATE TEMP TABLE order_item_ids (
   id UUID,
   refund_id UUID
);

INSERT INTO order_item_ids(id, refund_id)
SELECT oi.id, NULL
FROM order_items oi
INNER JOIN events e ON e.id = oi.event_id
INNER JOIN orders o ON oi.order_id = o.id
LEFT JOIN holds h ON oi.hold_id = h.id
LEFT JOIN order_items oi_promo_code ON (oi_promo_code.item_type = 'Discount' AND oi.id = oi_promo_code.parent_id)
WHERE ($1 IS NULL OR o.paid_at >= $1)
AND ($2 IS NULL OR o.paid_at <= $2)
AND (oi.item_type <> 'EventFees' OR oi.client_fee_in_cents > 0)
AND oi.item_type <> 'CreditCardFees'
AND e.organization_id = $5
AND o.status = 'Paid'
AND oi.parent_id IS NULL
AND ($3 IS NULL OR e.event_start >= $3)
AND ($4 IS NULL OR e.event_start <= $4);

-- Add refund items to the order items temp table
INSERT INTO order_item_ids(id, refund_id)
SELECT DISTINCT COALESCE(oi.parent_id, oi.id), r.id
FROM refunds r
INNER JOIN refund_items ri ON ri.refund_id = r.id
INNER JOIN order_items oi ON oi.id = ri.order_item_id
INNER JOIN events e ON e.id = oi.event_id
INNER JOIN orders o on oi.order_id = o.id
WHERE (oi.item_type <> 'EventFees' OR oi.client_fee_in_cents > 0)
AND oi.item_type <> 'CreditCardFees'
AND ($1 IS NULL OR r.created_at >= $1)
AND ($2 IS NULL OR r.created_at <= $2)
AND e.organization_id = $5
AND ri.amount > 0;

RETURN QUERY (SELECT COUNT(*) OVER ()                                                                                  AS total,
      r.*
      FROM (
          SELECT
              e.name                                                                                                   AS event_name,
              e.event_start                                                                                            AS event_date,
              CASE oi.item_type
                  WHEN 'EventFees' THEN 'Per Order Fee'
                  ELSE
                      concat(
                          CASE tt.status WHEN 'Cancelled' THEN concat(tt.name, ' (Cancelled)') ELSE tt.name END,
                          CASE
                              WHEN h.name IS NOT NULL THEN concat(' - Hold - ', h.name)
                              WHEN c.name IS NOT NULL THEN concat(' - Promo - ', c.name)
                              ELSE ''
                          END
                      )
              END                                                                                                      AS ticket_name,
              CASE oi.item_type
                  WHEN 'EventFees' THEN 0
                  ELSE CAST(oi.unit_price_in_cents + COALESCE(oi_promo_code.unit_price_in_cents, 0) AS BIGINT)
              END                                                                                                      AS face_value_in_cents,
              CAST(COALESCE(CASE oi.item_type
                WHEN 'EventFees' THEN 0
                ELSE
                  SUM(CASE WHEN oi_r.quantity IS NOT NULL THEN
                    -oi_r.quantity
                  ELSE
                    oi.quantity
                  END) FILTER (WHERE o.box_office_pricing IS FALSE AND
                                (h.hold_type IS NULL OR h.hold_type != 'Comp'))
              END, 0) AS BIGINT)                                                                                       AS online_sale_count,
              CAST(COALESCE(CASE oi.item_type
                  WHEN 'EventFees' THEN
                    SUM(CASE WHEN oi_r.quantity IS NOT NULL THEN
                      -oi_r.quantity * oi.client_fee_in_cents
                    ELSE
                      oi.quantity * oi.client_fee_in_cents
                    END) FILTER (WHERE o.box_office_pricing IS FALSE)
                  ELSE
                    SUM(CASE WHEN oi_t_fees_r.quantity IS NOT NULL THEN
                      -oi_t_fees_r.quantity * COALESCE(oi_t_fees.client_fee_in_cents, 0)
                    ELSE
                      COALESCE(oi_t_fees.quantity, 0) * COALESCE(oi_t_fees.client_fee_in_cents, 0)
                  END) FILTER (WHERE o.box_office_pricing IS FALSE)
              END, 0) AS BIGINT)                                                                                       AS total_online_client_fees_in_cents,
              CAST(COALESCE(CASE oi.item_type
                WHEN 'EventFees' THEN 0
                ELSE
                  SUM(CASE WHEN oi_r.quantity IS NOT NULL THEN
                    -oi_r.quantity
                  ELSE
                    oi.quantity
                  END) FILTER (WHERE o.box_office_pricing IS TRUE AND
                                (h.hold_type IS NULL OR h.hold_type != 'Comp'))
              END, 0) AS BIGINT)                                                                                       AS box_office_sale_count,
              CAST(COALESCE(SUM(
                  CASE WHEN oi_r.quantity IS NOT NULL THEN
                      -oi_r.quantity
                  ELSE
                    oi.quantity
                  END
              ) FILTER (WHERE h.hold_type = 'Comp'), 0) AS BIGINT)                                                     AS comp_sale_count
        FROM order_items oi
            INNER JOIN order_item_ids oi_ids ON oi.id = oi_ids.id
            INNER JOIN orders o ON oi.order_id = o.id AND o.status = 'Paid'
            INNER JOIN events e ON oi.event_id = e.id
            LEFT JOIN refund_items oi_r ON oi_r.order_item_id = oi.id AND oi_r.refund_id = oi_ids.refund_id
            LEFT JOIN order_items oi_promo_code ON (oi_promo_code.item_type = 'Discount' AND oi.id = oi_promo_code.parent_id)
            LEFT JOIN order_items oi_t_fees ON oi_t_fees.parent_id = oi.id AND oi_t_fees.item_type = 'PerUnitFees'
            LEFT JOIN refund_items oi_t_fees_r ON oi_t_fees_r.order_item_id = oi_t_fees.id AND oi_t_fees_r.refund_id = oi_ids.refund_id
            LEFT JOIN holds h ON oi.hold_id = h.id
            LEFT JOIN codes c ON oi.code_id = c.id
            LEFT JOIN ticket_types tt ON tt.id = oi.ticket_type_id
        GROUP BY e.id, e.event_start, tt.id, tt.name, tt.rank, oi.item_type, tt.status, oi.unit_price_in_cents, oi_promo_code.unit_price_in_cents, c.name, h.name
        ORDER BY e.event_start, tt.rank, tt.name, coalesce(h.name, c.name, '')
    ) r
-- Filter out any records where the sum of their quantities is 0
-- Negative indicates a refund adjustment, positive purchases
WHERE r.online_sale_count <> 0
    OR r.box_office_sale_count <> 0
    OR r.comp_sale_count <> 0
    OR r.total_online_client_fees_in_cents <> 0
LIMIT $7
OFFSET $6);

END $$ LANGUAGE 'plpgsql';
