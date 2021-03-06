CREATE OR REPLACE FUNCTION ticket_count_per_ticket_type(event_id UUID, organization_id UUID, group_by_event_id BOOLEAN, group_by_organization_id BOOLEAN)
  RETURNS TABLE
          (
            organization_id                      UUID,
            event_id                             UUID,
            ticket_type_id                       UUID,
            ticket_name                          TEXT,
            ticket_status                        TEXT,
            event_name                           TEXT,
            organization_name                    TEXT,
            allocation_count_including_nullified BIGINT,
            allocation_count                     BIGINT,
            unallocated_count                    BIGINT,
            reserved_count                       BIGINT,
            redeemed_count                       BIGINT,
            purchased_count                      BIGINT,
            nullified_count                      BIGINT,
            available_for_purchase_count         BIGINT,
            total_refunded_count                 BIGINT,
            comp_count                           BIGINT,
            comp_available_count                 BIGINT,
            comp_redeemed_count                  BIGINT,
            comp_purchased_count                 BIGINT,
            comp_reserved_count                  BIGINT,
            comp_nullified_count                 BIGINT,
            hold_count                           BIGINT,
            hold_available_count                 BIGINT,
            hold_redeemed_count                  BIGINT,
            hold_purchased_count                 BIGINT,
            hold_reserved_count                  BIGINT,
            hold_nullified_count                 BIGINT
          )
AS
$body$
SELECT o.id                                                                                                                                                                                              AS organization_id,
       e.id                                                                                                                                                                                              AS event_id,
       tt.id                                                                                                                                                                                             AS ticket_type_id,
       tt.name                                                                                                                                                                                           AS ticket_name,
       tt.status                                                                                                                                                                                         AS ticket_status,
       e.name                                                                                                                                                                                            AS event_name,
       o.name                                                                                                                                                                                            AS organization_name,

       -- Total Ticket Count
       CAST(COALESCE(COUNT(ti.id), 0) AS BIGINT)                                                                                                                                                         AS allocation_count_including_nullified,
       CAST(COALESCE(COUNT(ti.id) FILTER (WHERE ti.status != 'Nullified'), 0) AS BIGINT)                                                                                                                 AS allocation_count,
       CAST(COALESCE(COUNT(ti.id) FILTER (WHERE ti.status = 'Available' OR (ti.status = 'Reserved' AND ti.reserved_until < NOW())), 0) AS BIGINT)                                                        AS unallocated_count,

       CAST(COALESCE(COUNT(ti.id) FILTER (WHERE ti.status = 'Reserved' AND ti.reserved_until > NOW()), 0) AS BIGINT)                                                                                     AS reserved_count,
       CAST(COALESCE(COUNT(ti.id) FILTER (WHERE ti.status = 'Redeemed'), 0) AS BIGINT)                                                                                                                   AS redeemed_count,
       CAST(COALESCE(COUNT(ti.id) FILTER (WHERE ti.status = 'Purchased'), 0) AS BIGINT)                                                                                                                  AS purchased_count,
       CAST(COALESCE(COUNT(ti.id) FILTER (WHERE ti.status = 'Nullified'), 0) AS BIGINT)                                                                                                                  AS nullified_count,
       -- Not in a hold and not purchased / reserved / redeemed etc
       -- What can a generic user purchase.
       CAST(COALESCE(COUNT(ti.id) FILTER (WHERE ti.hold_id IS NULL AND (ti.status = 'Available' OR (ti.status = 'Reserved' AND ti.reserved_until < NOW()))), 0) AS BIGINT)                               AS available_for_purchase_count,
       --Refunded
       CAST(COUNT(rt.id) AS BIGINT)                                                                                                                                                                      AS total_refunded_count,
       -------------------- COMPS --------------------
       -- Comp counts
       CAST(COALESCE(COUNT(ti.id) FILTER (WHERE ti.hold_id IS NOT NULL AND h.hold_type = 'Comp'), 0) AS BIGINT)                                                                                          AS comp_count,
       CAST(COALESCE(COUNT(ti.id) FILTER (WHERE ti.hold_id IS NOT NULL AND h.hold_type = 'Comp' AND (ti.status = 'Available' OR (ti.status = 'Reserved' AND ti.reserved_until < NOW()))), 0) AS BIGINT)  AS comp_available_count,
       -- comp_count - comp_available_count = the sum of these
       CAST(COALESCE(COUNT(ti.id) FILTER (WHERE ti.hold_id IS NOT NULL AND h.hold_type = 'Comp' AND ti.status = 'Redeemed'), 0) AS BIGINT)                                                               AS comp_redeemed_count,
       CAST(COALESCE(COUNT(ti.id) FILTER (WHERE ti.hold_id IS NOT NULL AND h.hold_type = 'Comp' AND ti.status = 'Purchased'), 0) AS BIGINT)                                                              AS comp_purchased_count,
       CAST(COALESCE(COUNT(ti.id) FILTER (WHERE ti.hold_id IS NOT NULL AND h.hold_type = 'Comp' AND ti.status = 'Reserved' AND ti.reserved_until > NOW()), 0) AS BIGINT)                                 AS comp_reserved_count,
       CAST(COALESCE(COUNT(ti.id) FILTER (WHERE ti.hold_id IS NOT NULL AND h.hold_type = 'Comp' AND ti.status = 'Nullified'), 0) AS BIGINT)                                                              AS comp_nullified_count,
       ------------------ END COMPS ------------------

       -------------------- HOLDS --------------------
       -- Hold Counts
       CAST(COALESCE(COUNT(ti.id) FILTER (WHERE ti.hold_id IS NOT NULL AND h.hold_type != 'Comp'), 0) AS BIGINT)                                                                                         AS hold_count,
       CAST(COALESCE(COUNT(ti.id) FILTER (WHERE ti.hold_id IS NOT NULL AND h.hold_type != 'Comp' AND (ti.status = 'Available' OR (ti.status = 'Reserved' AND ti.reserved_until < NOW()))), 0) AS BIGINT) AS hold_available_count,
       -- hold_count - hold_available_count = the sum of these
       CAST(COALESCE(COUNT(ti.id) FILTER (WHERE ti.hold_id IS NOT NULL AND h.hold_type != 'Comp' AND ti.status = 'Redeemed'), 0) AS BIGINT)                                                              AS hold_redeemed_count,
       CAST(COALESCE(COUNT(ti.id) FILTER (WHERE ti.hold_id IS NOT NULL AND h.hold_type != 'Comp' AND ti.status = 'Purchased'), 0) AS BIGINT)                                                             AS hold_purchased_count,
       CAST(COALESCE(COUNT(ti.id) FILTER (WHERE ti.hold_id IS NOT NULL AND h.hold_type != 'Comp' AND ti.status = 'Reserved' AND ti.reserved_until > NOW()), 0) AS BIGINT)                                AS hold_reserved_count,
       CAST(COALESCE(COUNT(ti.id) FILTER (WHERE ti.hold_id IS NOT NULL AND h.hold_type != 'Comp' AND ti.status = 'Nullified'), 0) AS BIGINT)                                                             AS hold_nullified_count
       ------------------ END HOLDS -------------------
FROM ticket_instances ti
       LEFT JOIN holds h ON (h.id = ti.hold_id)
       LEFT JOIN refunded_tickets rt ON (rt.ticket_instance_id = ti.id)
       LEFT JOIN assets a ON (a.id = ti.asset_id)
       LEFT JOIN (SELECT tt.id, tt.name, tt.status FROM ticket_types tt WHERE $3 IS NOT TRUE AND $4 IS NOT TRUE) AS tt ON tt.id = a.ticket_type_id
       LEFT JOIN ticket_types tt2 ON a.ticket_type_id = tt2.id
       LEFT JOIN (SELECT e.id, e.organization_id, e.name FROM events e WHERE $4 IS NOT TRUE) AS e ON (e.id = tt2.event_id)
       LEFT JOIN events e2 ON (e2.id = tt2.event_id)
       LEFT JOIN organizations o ON o.id = e2.organization_id
WHERE ($1 IS NULL OR e.id = $1)
  AND ($2 IS NULL OR e.organization_id = $2)
GROUP BY e.id, e.name,o.id, o.name,tt.id, tt.name, tt.status;
$body$
  LANGUAGE SQL;
            
