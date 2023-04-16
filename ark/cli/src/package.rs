use clap::Parser;

#[derive(Clone, Debug, Parser)]
pub struct PackageFlags {
    #[arg(long, env = "ARK_FLAG_ADD_IF_NOT_EXISTS")]
    add_if_not_exists: bool,
}
