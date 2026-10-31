//! Typed, integrity-only references for RFC 0050. No stored field is authority.
use super::*;
use crate::registry::archive::AdmittedArchive;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct Locator {
    pub policy: String,
    pub name: String,
    pub version: String,
}

pub(crate) struct Reference {
    pub locator: Locator,
    pub binding: Policy,
    pub archive_sha256: String,
    pub archive_bytes: usize,
    target: usize,
    projection: [u8; 32],
}

/// Small derived metadata, not archive buffers or a persisted/trusted format.
pub(crate) struct ObjectSummary {
    pub bytes: usize,
    projections: [[u8; 32]; 2],
}

#[derive(Serialize)]
struct Projection<'a> {
    name: &'a str,
    version: &'a str,
    edition: &'a str,
    descriptor_sha256: &'a str,
    sources: Vec<(&'a str, &'a str)>,
    dependencies: Vec<(&'a str, &'a str, &'a str, &'a str, &'a str)>,
}

fn project(value: &Projection<'_>) -> Result<[u8; 32], Diagnostic> {
    let bytes = serde_json::to_vec(value).map_err(|_| capacity())?;
    Ok(Sha256::digest(bytes).into())
}

impl ObjectSummary {
    pub fn capture(archive: &AdmittedArchive) -> Result<Self, Diagnostic> {
        let descriptor = archive.descriptor();
        let package = descriptor.package();
        let mut projections = [[0; 32]; 2];
        for (i, target) in manifest::TARGETS.iter().enumerate() {
            projections[i] = project(&Projection {
                name: package.name(),
                version: package.version(),
                edition: package.edition(),
                descriptor_sha256: archive.descriptor_sha256(),
                sources: descriptor
                    .files()
                    .iter()
                    .map(|f| (f.path(), f.sha256()))
                    .collect(),
                dependencies: descriptor
                    .dependencies()
                    .iter()
                    .filter(|d| d.targets().iter().any(|t| t == target))
                    .map(|d| {
                        (
                            d.alias(),
                            d.registry(),
                            d.name(),
                            d.version(),
                            d.archive_sha256(),
                        )
                    })
                    .collect(),
            })?;
        }
        Ok(Self {
            bytes: archive.as_bytes().len(),
            projections,
        })
    }
}

impl Reference {
    pub fn matches(&self, summary: &ObjectSummary) -> bool {
        self.archive_bytes == summary.bytes && self.projection == summary.projections[self.target]
    }
}

pub(crate) fn references(bytes: &[u8]) -> Result<Vec<Reference>, Diagnostic> {
    // Reuse the closed bounded canonical admission; do not flatten foreign JSON.
    super::super::validate_encoding(bytes)?;
    let record: Record = serde_json::from_slice(bytes)
        .map_err(|_| error("cache generation requires lock schema 3"))?;
    if record.schema != 3 {
        return Err(error("cache generation requires lock schema 3"));
    }
    let target = manifest::TARGETS
        .iter()
        .position(|t| *t == record.target)
        .ok_or_else(|| error("unsupported cache target"))?;
    let mut result = Vec::new();
    for package in &record.packages {
        let Origin::Registry {
            registry,
            origin,
            root_sha256,
            archive_sha256,
            archive_bytes,
            descriptor_sha256,
        } = &package.origin
        else {
            continue;
        };
        let mut root = [0; 32];
        for (slot, pair) in root
            .iter_mut()
            .zip(root_sha256.as_bytes().as_chunks::<2>().0)
        {
            let text = std::str::from_utf8(pair).map_err(|_| error("invalid root digest"))?;
            *slot = u8::from_str_radix(text, 16).map_err(|_| error("invalid root digest"))?;
        }
        let identity = ServiceIdentity::new(registry, origin)
            .map_err(|_| error("invalid registry binding"))?;
        let binding = Policy::new(identity, root);
        let prefix = format!("{}/", package.id);
        let sources = package
            .sources
            .iter()
            .map(|s| {
                s.path
                    .strip_prefix(&prefix)
                    .map(|p| (p, s.sha256.as_str()))
                    .ok_or_else(|| error("source outside registry namespace"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut dependencies = Vec::new();
        for edge in &package.dependencies {
            let index = record
                .packages
                .binary_search_by(|p| p.id.cmp(&edge.id))
                .map_err(|_| error("missing registry dependency"))?;
            let dependency = &record.packages[index];
            let Origin::Registry {
                registry,
                archive_sha256,
                ..
            } = &dependency.origin
            else {
                return Err(error("registry dependency has local origin"));
            };
            dependencies.push((
                edge.alias.as_str(),
                registry.as_str(),
                dependency.name.as_str(),
                dependency.version.as_str(),
                archive_sha256.as_str(),
            ));
        }
        let projection = project(&Projection {
            name: &package.name,
            version: &package.version,
            edition: &package.edition,
            descriptor_sha256,
            sources,
            dependencies,
        })?;
        result.push(Reference {
            locator: Locator {
                policy: binding.policy_id().to_owned(),
                name: package.name.clone(),
                version: package.version.clone(),
            },
            binding,
            archive_sha256: archive_sha256.clone(),
            archive_bytes: usize::try_from(*archive_bytes).map_err(|_| capacity())?,
            target,
            projection,
        });
    }
    Ok(result)
}
