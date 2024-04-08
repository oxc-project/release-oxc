use std::path::PathBuf;

use anyhow::Result;
use bpaf::Bpaf;
use cargo_metadata::{Metadata, MetadataCommand, Package};

use crate::cargo_command::CargoCommand;

#[derive(Debug, Clone, Bpaf)]
pub struct Options {
    #[bpaf(positional("PATH"), fallback(PathBuf::from(".")))]
    path: PathBuf,
}

pub struct Publish {
    metadata: Metadata,
    cargo: CargoCommand,
}

impl Publish {
    /// # Errors
    #[allow(clippy::needless_pass_by_value)]
    pub fn new(options: Options) -> Result<Self> {
        let metadata = MetadataCommand::new().current_dir(&options.path).no_deps().exec()?;
        let cargo = CargoCommand::new(metadata.workspace_root.clone().into_std_path_buf());
        Ok(Self { metadata, cargo })
    }

    /// # Errors
    pub fn run(self) -> Result<()> {
        let packages = self.get_packages();
        let packages = release_order::release_order(&packages)?;
        let packages = packages.into_iter().map(|package| &package.name).collect::<Vec<_>>();

        println!("Checking");
        self.cargo.run(&["check", "--all-features", "--all-targets"])?;

        println!("Publishing packages: {packages:?}");
        for package in &packages {
            self.cargo.publish(package)?;
        }
        println!("Published packages: {packages:?}");
        Ok(())
    }

    fn get_packages(&self) -> Vec<&Package> {
        // `publish.is_none()` means `publish = true`.
        self.metadata.workspace_packages().into_iter().filter(|p| p.publish.is_none()).collect()
    }
}

mod release_order {
    use anyhow::Result;
    use cargo_metadata::Package;

    /// Return packages in an order they can be released.
    /// In the result, the packages are placed after all their dependencies.
    /// Return an error if a circular dependency is detected.
    pub fn release_order<'a>(packages: &'a [&Package]) -> Result<Vec<&'a Package>> {
        let mut order = vec![];
        let mut passed = vec![];
        for p in packages {
            release_order_inner(packages, p, &mut order, &mut passed)?;
        }
        Ok(order)
    }

    /// The `passed` argument is used to track packages that you already visited to
    /// detect circular dependencies.
    fn release_order_inner<'a>(
        packages: &[&'a Package],
        pkg: &'a Package,
        order: &mut Vec<&'a Package>,
        passed: &mut Vec<&'a Package>,
    ) -> Result<()> {
        if is_package_in(pkg, order) {
            return Ok(());
        }
        passed.push(pkg);

        for d in &pkg.dependencies {
            // Check if the dependency is part of the packages we are releasing.
            if let Some(dep) = packages.iter().find(|p| {
                d.name == p.name
              // Exclude the current package.
              && p.name != pkg.name
            }) {
                anyhow::ensure!(
                    !is_package_in(dep, passed),
                    "Circular dependency detected: {} -> {}",
                    dep.name,
                    pkg.name,
                );
                release_order_inner(packages, dep, order, passed)?;
            }
        }

        order.push(pkg);
        passed.clear();
        Ok(())
    }

    /// Return true if the package is part of a packages array.
    /// This function exists because `package.contains(pkg)` is expensive,
    /// because it compares the whole package struct.
    fn is_package_in(pkg: &Package, packages: &[&Package]) -> bool {
        packages.iter().any(|p| p.name == pkg.name)
    }
}
