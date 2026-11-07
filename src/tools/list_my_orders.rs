//! `list_my_orders` → `mostro-cli order mine`: the orders this identity holds
//! in the CLI's local store (made, taken or adopted by `sync_orders`), with
//! their local status. Read-only and offline: no relay, no node.

pub fn args() -> Vec<String> {
    vec!["order".to_string(), "mine".to_string()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asks_order_mine() {
        assert_eq!(args(), ["order", "mine"]);
    }
}
