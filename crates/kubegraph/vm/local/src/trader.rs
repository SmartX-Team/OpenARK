#[cfg(feature = "trader-default")]
pub type NetworkTrader = ::kubegraph_trader::NetworkTrader;
#[cfg(not(feature = "trader-default"))]
pub type NetworkTrader = ();
