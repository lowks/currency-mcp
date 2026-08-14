use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{
        router::{prompt::PromptRouter, tool::ToolRouter},
        wrapper::Parameters,
    },
    model::*,
    prompt, prompt_handler, prompt_router, schemars,
    service::RequestContext,
    tool, tool_handler, tool_router,
};
use serde::{Deserialize, Serialize};

use crate::client::{ClientError, FrankfurterClient};
use crate::types::deserialize_f64_from_string_or_number;

#[derive(Clone)]
pub struct CurrencyServer {
    client: FrankfurterClient,
    #[allow(dead_code)]
    tool_router: ToolRouter<CurrencyServer>,
    #[allow(dead_code)]
    prompt_router: PromptRouter<CurrencyServer>,
}

impl CurrencyServer {
    pub fn new(client: FrankfurterClient) -> Self {
        Self {
            client,
            tool_router: Self::tool_router(),
            prompt_router: Self::prompt_router(),
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ListCurrenciesParams {
    /// Optional case-insensitive filter matched against ISO codes and currency names.
    query: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct CurrencyCodeParams {
    /// ISO 4217 currency code, for example USD, EUR, or JPY.
    code: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct LatestRatesParams {
    /// Base currency ISO code. Defaults to USD.
    base: Option<String>,
    /// Comma-separated quote currencies, for example "EUR,GBP,JPY". Omit to return all quotes.
    quotes: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct RateParams {
    /// Source currency ISO code.
    from: String,
    /// Target currency ISO code.
    to: String,
    /// Optional ISO date (YYYY-MM-DD). Defaults to the latest available rate.
    date: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ConvertParams {
    /// Amount of money in the source currency.
    amount: f64,
    /// Source currency ISO code.
    from: String,
    /// Target currency ISO code.
    to: String,
    /// Optional ISO date (YYYY-MM-DD). Defaults to the latest available rate.
    date: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct HistoricalRatesParams {
    /// ISO date (YYYY-MM-DD) to look up.
    date: String,
    /// Base currency ISO code. Defaults to USD.
    base: Option<String>,
    /// Comma-separated quote currencies. Omit to return all quotes for that date.
    quotes: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct RateHistoryParams {
    /// Inclusive start date (YYYY-MM-DD).
    from_date: String,
    /// Inclusive end date (YYYY-MM-DD).
    to_date: String,
    /// Base currency ISO code. Defaults to USD.
    base: Option<String>,
    /// Comma-separated quote currencies. Required so the time series stays small.
    quotes: String,
    /// Optional downsample: "week" or "month".
    group: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
struct ConvertMoneyPromptArgs {
    /// Amount of money to convert.
    #[serde(deserialize_with = "deserialize_f64_from_string_or_number")]
    amount: f64,
    /// Source currency ISO code, for example USD.
    from: String,
    /// Target currency ISO code, for example EUR.
    to: String,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
struct TravelBudgetPromptArgs {
    /// Traveler's home currency ISO code.
    home_currency: String,
    /// Destination currency ISO code.
    destination_currency: String,
    /// Trip budget in the home currency.
    #[serde(deserialize_with = "deserialize_f64_from_string_or_number")]
    budget: f64,
    /// Optional trip context, for example "10 days in Tokyo, mid-range".
    context: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
struct CompareCurrenciesPromptArgs {
    /// Base currency ISO code.
    base: String,
    /// Comma-separated quote currencies to compare against the base.
    quotes: String,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
struct FxBriefingPromptArgs {
    /// Base currency for the briefing, for example USD.
    base: String,
    /// Optional comma-separated currencies to focus on.
    quotes: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
struct HistoricalMovePromptArgs {
    /// Source currency ISO code.
    from: String,
    /// Target currency ISO code.
    to: String,
    /// Inclusive start date (YYYY-MM-DD).
    from_date: String,
    /// Inclusive end date (YYYY-MM-DD).
    to_date: String,
}

#[tool_router]
impl CurrencyServer {
    #[tool(
        description = "List world currencies with ISO codes, names, and symbols. Optionally filter by code or name."
    )]
    async fn list_currencies(
        &self,
        Parameters(params): Parameters<ListCurrenciesParams>,
    ) -> Result<CallToolResult, McpError> {
        match self.client.list_currencies(params.query.as_deref()).await {
            Ok(currencies) => json_ok(&currencies),
            Err(error) => map_client_error(error),
        }
    }

    #[tool(description = "Get details for one currency, including provider coverage.")]
    async fn get_currency(
        &self,
        Parameters(params): Parameters<CurrencyCodeParams>,
    ) -> Result<CallToolResult, McpError> {
        match self.client.get_currency(&params.code).await {
            Ok(currency) => json_ok(&currency),
            Err(error) => map_client_error(error),
        }
    }

    #[tool(
        description = "Get the latest exchange rates for a base currency against one or more quote currencies."
    )]
    async fn get_latest_rates(
        &self,
        Parameters(params): Parameters<LatestRatesParams>,
    ) -> Result<CallToolResult, McpError> {
        let base = params.base.as_deref().unwrap_or("USD");
        match self
            .client
            .get_rates(base, params.quotes.as_deref(), None, None, None, None)
            .await
        {
            Ok(rates) => json_ok(&rates),
            Err(error) => map_client_error(error),
        }
    }

    #[tool(
        description = "Get the exchange rate for a single currency pair. Pass date (YYYY-MM-DD) for a historical rate."
    )]
    async fn get_rate(
        &self,
        Parameters(params): Parameters<RateParams>,
    ) -> Result<CallToolResult, McpError> {
        match self
            .client
            .get_rate(&params.from, &params.to, params.date.as_deref())
            .await
        {
            Ok(rate) => json_ok(&rate),
            Err(error) => map_client_error(error),
        }
    }

    #[tool(
        description = "Convert an amount from one currency to another using the latest or a historical daily rate."
    )]
    async fn convert_currency(
        &self,
        Parameters(params): Parameters<ConvertParams>,
    ) -> Result<CallToolResult, McpError> {
        match self
            .client
            .convert(
                params.amount,
                &params.from,
                &params.to,
                params.date.as_deref(),
            )
            .await
        {
            Ok(conversion) => json_ok(&conversion),
            Err(error) => map_client_error(error),
        }
    }

    #[tool(description = "Get exchange rates for a specific historical date.")]
    async fn get_historical_rates(
        &self,
        Parameters(params): Parameters<HistoricalRatesParams>,
    ) -> Result<CallToolResult, McpError> {
        let base = params.base.as_deref().unwrap_or("USD");
        match self
            .client
            .get_rates(
                base,
                params.quotes.as_deref(),
                Some(&params.date),
                None,
                None,
                None,
            )
            .await
        {
            Ok(rates) => json_ok(&rates),
            Err(error) => map_client_error(error),
        }
    }

    #[tool(
        description = "Get a time series of daily exchange rates between two dates. Pass group=week or group=month to downsample."
    )]
    async fn get_rate_history(
        &self,
        Parameters(params): Parameters<RateHistoryParams>,
    ) -> Result<CallToolResult, McpError> {
        let base = params.base.as_deref().unwrap_or("USD");
        match self
            .client
            .get_rates(
                base,
                Some(&params.quotes),
                None,
                Some(&params.from_date),
                Some(&params.to_date),
                params.group.as_deref(),
            )
            .await
        {
            Ok(rates) => json_ok(&rates),
            Err(error) => map_client_error(error),
        }
    }
}

#[prompt_router]
impl CurrencyServer {
    #[prompt(
        name = "convert_money",
        description = "Prompt the assistant to convert an amount between two world currencies and explain the result."
    )]
    async fn convert_money(
        &self,
        Parameters(args): Parameters<ConvertMoneyPromptArgs>,
    ) -> Result<GetPromptResult, McpError> {
        let messages = vec![
            PromptMessage::new_text(
                Role::Assistant,
                "You are a world currency exchange assistant. Use the convert_currency tool for live or dated rates before answering. Rates come from central-bank reference data via Frankfurter, not live tradable quotes. State the date, rate, and converted amount clearly.",
            ),
            PromptMessage::new_text(
                Role::User,
                format!(
                    "Convert {amount} {from} to {to}. Fetch the current rate, show the math, and give a one-sentence takeaway about what that rate means for someone exchanging money today.",
                    amount = args.amount,
                    from = args.from.to_ascii_uppercase(),
                    to = args.to.to_ascii_uppercase()
                ),
            ),
        ];
        Ok(GetPromptResult::new(messages).with_description(format!(
            "Convert {} {} to {}",
            args.amount, args.from, args.to
        )))
    }

    #[prompt(
        name = "travel_budget",
        description = "Prompt the assistant to turn a home-currency travel budget into destination-currency spending guidance."
    )]
    async fn travel_budget(
        &self,
        Parameters(args): Parameters<TravelBudgetPromptArgs>,
    ) -> Result<GetPromptResult, McpError> {
        let context = args.context.as_deref().unwrap_or("general leisure travel");
        let messages = vec![
            PromptMessage::new_text(
                Role::Assistant,
                "You help travelers plan spending across currencies. Always call convert_currency and get_latest_rates before giving advice. Break the budget into cash, cards, and a small buffer. Flag that reference rates are not what a booth or bank will charge.",
            ),
            PromptMessage::new_text(
                Role::User,
                format!(
                    "I have {budget} {home} for a trip that will be spent in {dest}. Trip context: {context}. Convert the budget, suggest a practical daily split, and mention typical extra costs like ATM or card FX fees in general terms.",
                    budget = args.budget,
                    home = args.home_currency.to_ascii_uppercase(),
                    dest = args.destination_currency.to_ascii_uppercase(),
                    context = context
                ),
            ),
        ];
        Ok(GetPromptResult::new(messages).with_description(format!(
            "Travel budget {} {} -> {}",
            args.budget, args.home_currency, args.destination_currency
        )))
    }

    #[prompt(
        name = "compare_currencies",
        description = "Prompt the assistant to compare several world currencies against a base currency."
    )]
    async fn compare_currencies(
        &self,
        Parameters(args): Parameters<CompareCurrenciesPromptArgs>,
    ) -> Result<GetPromptResult, McpError> {
        let messages = vec![
            PromptMessage::new_text(
                Role::Assistant,
                "You compare world currencies using official reference rates. Call get_latest_rates with the requested base and quotes. Present a compact table, then a short relative-strength summary. Do not give investment advice.",
            ),
            PromptMessage::new_text(
                Role::User,
                format!(
                    "Compare {base} against {quotes}. Fetch the latest rates and explain which quotes buy more or less of the base today.",
                    base = args.base.to_ascii_uppercase(),
                    quotes = args.quotes.to_ascii_uppercase()
                ),
            ),
        ];
        Ok(GetPromptResult::new(messages)
            .with_description(format!("Compare {} vs {}", args.base, args.quotes)))
    }

    #[prompt(
        name = "fx_briefing",
        description = "Prompt a concise world FX briefing for a base currency."
    )]
    async fn fx_briefing(
        &self,
        Parameters(args): Parameters<FxBriefingPromptArgs>,
    ) -> Result<GetPromptResult, McpError> {
        let quotes = args
            .quotes
            .clone()
            .unwrap_or_else(|| "EUR,GBP,JPY,CNY,AUD,CAD,CHF,INR".to_string());
        let messages = vec![
            PromptMessage::new_text(
                Role::Assistant,
                "You write short FX briefings from central-bank reference rates. Use get_latest_rates, and list_currencies only if a code is unfamiliar. Keep the briefing to a headline, a rate table, and 3 takeaways. No trading recommendations.",
            ),
            PromptMessage::new_text(
                Role::User,
                format!(
                    "Give me today's world currency briefing with base {base}, focusing on {quotes}.",
                    base = args.base.to_ascii_uppercase(),
                    quotes = quotes.to_ascii_uppercase()
                ),
            ),
        ];
        Ok(GetPromptResult::new(messages)
            .with_description(format!("FX briefing for {}", args.base)))
    }

    #[prompt(
        name = "historical_move",
        description = "Prompt the assistant to explain how a currency pair moved between two dates."
    )]
    async fn historical_move(
        &self,
        Parameters(args): Parameters<HistoricalMovePromptArgs>,
    ) -> Result<GetPromptResult, McpError> {
        let messages = vec![
            PromptMessage::new_text(
                Role::Assistant,
                "You explain historical currency moves with data. Call get_rate for the start and end dates, and get_rate_history for the range. Report start rate, end rate, percent change, and a cautious summary. These are reference rates, not tradable prices.",
            ),
            PromptMessage::new_text(
                Role::User,
                format!(
                    "How did {from}/{to} move from {start} to {end}? Fetch the data and summarize the change.",
                    from = args.from.to_ascii_uppercase(),
                    to = args.to.to_ascii_uppercase(),
                    start = args.from_date,
                    end = args.to_date
                ),
            ),
        ];
        Ok(GetPromptResult::new(messages).with_description(format!(
            "{}{} from {} to {}",
            args.from, args.to, args.from_date, args.to_date
        )))
    }
}

#[tool_handler]
#[prompt_handler]
impl ServerHandler for CurrencyServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_prompts()
                .enable_tools()
                .build(),
        )
        .with_server_info(Implementation::from_build_env())
        .with_instructions(
            "World currency exchange MCP server. Tools fetch reference FX rates from Frankfurter (central-bank data, 200+ currencies, no API key). Use convert_currency, get_latest_rates, get_rate, get_historical_rates, get_rate_history, list_currencies, and get_currency before answering money questions. Prompts: convert_money, travel_budget, compare_currencies, fx_briefing, historical_move. Always state the rate date and that these are reference rates, not live tradable quotes."
                .to_string(),
        )
    }

    async fn initialize(
        &self,
        _request: InitializeRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, McpError> {
        Ok(self.get_info())
    }
}

fn json_ok<T: Serialize>(value: &T) -> Result<CallToolResult, McpError> {
    let text = serde_json::to_string_pretty(value)
        .map_err(|error| McpError::internal_error(error.to_string(), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
}

fn map_client_error(error: ClientError) -> Result<CallToolResult, McpError> {
    match error {
        ClientError::InvalidCurrency(_)
        | ClientError::InvalidDate(_)
        | ClientError::InvalidParam(_) => Err(McpError::invalid_params(error.to_string(), None)),
        other => Ok(CallToolResult::error(vec![ContentBlock::text(
            other.to_string(),
        )])),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_router_registers_fx_tools() {
        let router = CurrencyServer::tool_router();
        for name in [
            "list_currencies",
            "get_currency",
            "get_latest_rates",
            "get_rate",
            "convert_currency",
            "get_historical_rates",
            "get_rate_history",
        ] {
            assert!(router.has_route(name), "missing tool {name}");
        }
    }

    #[test]
    fn prompt_router_registers_fx_prompts() {
        let router = CurrencyServer::prompt_router();
        for name in [
            "convert_money",
            "travel_budget",
            "compare_currencies",
            "fx_briefing",
            "historical_move",
        ] {
            assert!(router.has_route(name), "missing prompt {name}");
        }
        assert_eq!(router.list_all().len(), 5);
    }

    #[test]
    fn server_info_enables_tools_and_prompts() {
        let client = FrankfurterClient::new().unwrap();
        let info = CurrencyServer::new(client).get_info();
        assert!(info.capabilities.tools.is_some());
        assert!(info.capabilities.prompts.is_some());
        assert!(
            info.instructions
                .as_deref()
                .is_some_and(|text| text.contains("convert_currency"))
        );
    }
}
