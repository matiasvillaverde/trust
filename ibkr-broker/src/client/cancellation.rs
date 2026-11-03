//! Confirm cancellation using the selected account's documented status endpoint.
use super::*;

pub(super) fn confirm(
    client: &IbkrClient,
    account: &Account,
    order_id: &str,
    order_ref: &str,
) -> Result<(), Box<dyn Error>> {
    let account_id = crate::support::broker_account_id(account)?;
    let path = format!("/iserver/account/order/status/{order_id}");
    for _ in 0..LIVE_ORDER_LOOKUP_RETRIES {
        let order = client.get_json_value(&path, &[])?;
        if string_field_optional(&order, "account").as_deref() != Some(account_id)
            || string_field_optional(&order, "order_id").as_deref() != Some(order_id)
        {
            return Err(
                "IBKR cancellation status identity mismatch; reconcile before releasing capital"
                    .into(),
            );
        }
        let status = string_field_optional(&order, "status")
            .or_else(|| string_field_optional(&order, "order_status"))
            .unwrap_or_default()
            .to_ascii_lowercase();
        if matches!(
            status.as_str(),
            "cancelled" | "canceled" | "api_cancelled" | "apicancelled"
        ) {
            let filled_quantity = decimal_field_optional_any(
                &order,
                &[
                    "cum_fill",
                    "cumFill",
                    "filledQuantity",
                    "filled_quantity",
                    "filled",
                ],
            );
            if filled_quantity == Some(Decimal::ZERO) {
                return Ok(());
            }
            return Err(format!(
                    "IBKR order '{order_ref}' was canceled but its filled quantity was nonzero or unavailable; funds must not be released; reconcile executions"
                )
                .into());
        }
        if matches!(status.as_str(), "filled" | "partially_filled") {
            return Err(format!(
                    "IBKR order '{order_ref}' became '{status}' while cancellation was pending; funds must not be released"
                )
                .into());
        }
        sleep(Duration::from_millis(LIVE_ORDER_LOOKUP_DELAY_MS));
    }
    Err(format!(
        "IBKR did not confirm cancellation of order '{order_ref}'; funds must not be released"
    )
    .into())
}
