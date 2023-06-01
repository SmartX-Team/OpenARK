use std::collections::BTreeMap;

use anyhow::{anyhow, bail, Result};
use futures::future::try_join_all;
use k8s_openapi::{
    api::core::v1::{
        CSIPersistentVolumeSource, PersistentVolume, PersistentVolumeClaim,
        PersistentVolumeClaimSpec, PersistentVolumeSpec, Secret, SecretReference,
    },
    Metadata,
};
use kube::{
    api::{ListParams, PostParams},
    core::ObjectMeta,
    Api, Client, ResourceExt,
};

pub(crate) mod consts {
    pub const NAME: &str = "vine-storage";
    pub const NAMESPACE_SHARED: &str = "vine-guest";

    pub const SECRET_ROOK_CSI_CEPHFS_NODE_NAME: &str = "rook-csi-cephfs-node";
    pub const SECRET_ROOK_CSI_CEPHFS_USER_NAME: &str = "rook-csi-cephfs-user";
    pub const PV_PERSISTENT_VOLUME_RECLAIM_POLICY: &str = "Retain";
}

pub async fn get_or_create_shared_pvcs(
    kube: &Client,
    target_namespace: &str,
) -> Result<Vec<PersistentVolumeClaim>> {
    // search sharable PVCs
    let source_namespace = self::consts::NAMESPACE_SHARED;
    let api = Api::namespaced(kube.clone(), source_namespace);
    let lp = ListParams {
        label_selector: Some("vine.ulagbulag.io/shared=true".into()),
        ..Default::default()
    };
    match api.list(&lp).await {
        Ok(pvcs) => {
            try_join_all(
                pvcs.into_iter()
                    .map(|pvc| clone_pvc(kube, source_namespace, target_namespace, pvc)),
            )
            .await
        }
        Err(error) => {
            bail!("failed to get shared PVCs ({source_namespace} => {target_namespace}): {error}")
        }
    }
}

async fn clone_pvc(
    kube: &Client,
    source_namespace: &str,
    target_namespace: &str,
    pvc: PersistentVolumeClaim,
) -> Result<PersistentVolumeClaim> {
    // skip creating if the PVC already exists
    let target_api = Api::<PersistentVolumeClaim>::namespaced(kube.clone(), target_namespace);
    let name = pvc.name_any();
    if let Some(pvc) = target_api.get_opt(&name).await? {
        return Ok(pvc);
    }

    // get original PVC
    let pv_name = match pvc.spec.as_ref().and_then(|spec| spec.volume_name.as_ref()) {
        Some(pv_name) => pv_name,
        None => bail!("shared PVC is not ready: {source_namespace}/{name}"),
    };

    let pp = PostParams {
        field_manager: Some(self::consts::NAME.into()),
        ..Default::default()
    };

    // try to clone PV
    let pv = clone_pv(kube, target_namespace, pv_name, &pp).await?;

    let ObjectMeta {
        annotations,
        labels,
        ..
    } = pvc.metadata;
    let PersistentVolumeClaimSpec {
        access_modes,
        resources,
        storage_class_name,
        volume_mode,
        ..
    } = pvc.spec.unwrap_or_default();

    let pvc = PersistentVolumeClaim {
        metadata: ObjectMeta {
            annotations,
            labels,
            name: Some(name.clone()),
            namespace: Some(target_namespace.into()),
            ..Default::default()
        },
        spec: Some(PersistentVolumeClaimSpec {
            access_modes,
            storage_class_name,
            resources,
            volume_mode,
            volume_name: Some(pv.name_any()),
            ..Default::default()
        }),
        status: None,
    };

    target_api
        .create(&pp, &pvc)
        .await
        .map_err(|error| anyhow!(
            "failed to create a PVC ({source_namespace}/{name} => {target_namespace}/{name}): {error}",
        ))
}

/// Copy the YAML content of the PV, and create a new static PV
/// with the same information and some modifications.
///
/// From the original YAML, it must:
///
/// 1. Modify the original name. To keep track, the best solution is
///    to append to the original name the namespace name
/// where you want your new PV.
/// 2. Modify the volumeHandle. Again append the targeted namespace.
/// 3. Add the `staticVolume: "true"` entry to the volumeAttributes.
/// 4. Add the rootPath entry to the volumeAttributes,
///    with the same content as subvolumePath.
/// 5. In the `nodeStageSecretRef` section, change the name to point
///    to the secret you created earlier.
/// 6. Remove the unnecessary information before applying the YAML
///    (claimRef, managedFields,...)
///
/// * Reference: https://rook.io/docs/rook/v1.11/Storage-Configuration/Shared-Filesystem-CephFS/filesystem-storage/#shared-volume-creation
async fn clone_pv(
    kube: &Client,
    target_namespace: &str,
    source_name: &str,
    pp: &PostParams,
) -> Result<PersistentVolume> {
    // skip creating if the PV already exists
    let api = Api::<PersistentVolume>::all(kube.clone());
    let target_name = format!("{source_name}-{target_namespace}");
    if let Some(pv) = api.get_opt(&target_name).await? {
        return Ok(pv);
    }

    // get original PV
    let pv = match api.get(source_name).await {
        Ok(pv) => pv,
        Err(e) => {
            bail!("failed to find a shared PV ({source_name}): {e}")
        }
    };
    let pv = retain_pv_on_delete(&api, pv, pp).await?;

    let secret_ref = get_or_create_user_level_cephfs_secret(kube, &pv, pp).await?;

    let ObjectMeta {
        annotations,
        labels,
        ..
    } = pv.metadata;
    let PersistentVolumeSpec {
        access_modes,
        capacity,
        csi,
        persistent_volume_reclaim_policy,
        storage_class_name,
        volume_mode,
        ..
    } = pv.spec.unwrap_or_default();
    let CSIPersistentVolumeSource {
        driver,
        volume_attributes,
        volume_handle,
        ..
    } = csi.unwrap_or_default();

    let pv = PersistentVolume {
        metadata: ObjectMeta {
            annotations,
            labels,
            name: Some(target_name.clone()),
            ..Default::default()
        },
        spec: Some(PersistentVolumeSpec {
            access_modes,
            capacity,
            csi: Some(CSIPersistentVolumeSource {
                driver,
                node_stage_secret_ref: Some(secret_ref),
                volume_attributes: Some({
                    let mut volume_attributes = volume_attributes.unwrap_or_default();
                    volume_attributes.insert(
                        "rootPath".into(),
                        volume_attributes
                            .get("subvolumePath")
                            .cloned()
                            .unwrap_or_default(),
                    );
                    volume_attributes.insert("staticVolume".into(), "true".into());
                    volume_attributes
                }),
                volume_handle: format!("{volume_handle}-{target_namespace}"),
                ..Default::default()
            }),
            persistent_volume_reclaim_policy,
            storage_class_name,
            volume_mode,
            ..Default::default()
        }),
        status: None,
    };

    api.create(pp, &pv)
        .await
        .map_err(|error| anyhow!("failed to create a PV ({source_name} => {target_name}): {error}"))
}

/// On this PV, change the `persistentVolumeReclaimPolicy` parameter to `Retain`
/// to avoid it from being deleted when you will delete PVCs.
///
/// Don't forget to change it back to `Delete` when you want to remove the shared volume.
async fn retain_pv_on_delete(
    api: &Api<PersistentVolume>,
    mut pv: PersistentVolume,
    pp: &PostParams,
) -> Result<PersistentVolume> {
    if let Some(spec) = &mut pv.spec {
        // skip if already patched
        if spec.persistent_volume_reclaim_policy.as_deref()
            == Some(self::consts::PV_PERSISTENT_VOLUME_RECLAIM_POLICY)
        {
            return Ok(pv);
        }

        // apply patch
        spec.persistent_volume_reclaim_policy =
            Some(self::consts::PV_PERSISTENT_VOLUME_RECLAIM_POLICY.into());
    }

    // save
    let name = pv.name_any();
    match api.replace(&name, pp, &pv).await {
        Ok(_) => Ok(pv),
        Err(error) => {
            bail!("failed to update the PV ({name}): {error}")
        }
    }
}

async fn get_or_create_user_level_cephfs_secret(
    kube: &Client,
    pv: &PersistentVolume,
    pp: &PostParams,
) -> Result<SecretReference> {
    fn get_secret_ref(secret: Secret) -> SecretReference {
        SecretReference {
            name: secret.metadata.name,
            namespace: secret.metadata.namespace,
        }
    }

    let pv_name = pv.name_any();
    if pv
        .metadata()
        .annotations
        .as_ref()
        .and_then(|annotations| annotations.get("pv.kubernetes.io/provisioned-by"))
        .filter(|provisioner| provisioner.ends_with(".cephfs.csi.ceph.com"))
        .is_none()
    {
        bail!("unsupported PV sharing; Only the Rook CephFS is supported: {pv_name}")
    }

    let namespace = match pv
        .spec
        .as_ref()
        .and_then(|spec| spec.csi.as_ref())
        .and_then(|csi| csi.controller_expand_secret_ref.as_ref())
        .and_then(|controller_expand_secret_ref| controller_expand_secret_ref.namespace.as_ref())
    {
        Some(namespace) => namespace,
        None => bail!("PV's CSI ControllerExpandSecretRef is missing: {pv_name}"),
    };

    // skip creating if the secret already exists
    let api = Api::namespaced(kube.clone(), namespace);
    let target_name = self::consts::SECRET_ROOK_CSI_CEPHFS_USER_NAME;
    if let Some(secret) = api.get_opt(target_name).await? {
        return Ok(get_secret_ref(secret));
    }

    // get original secret
    let api = Api::<Secret>::namespaced(kube.clone(), namespace);
    let source_name = self::consts::SECRET_ROOK_CSI_CEPHFS_NODE_NAME;
    let secret = match api.get(source_name).await {
        Ok(secret) => secret,
        Err(error) => {
            bail!("failed to find a Rook CephFS node secret ({namespace}/{source_name}): {error}")
        }
    };

    let get_data = |key: &str| match secret.data.as_ref().and_then(|data| data.get(key)) {
        Some(value) => Ok(value.clone()),
        None => {
            bail!("failed to find a Rook CephFS node secret data key: {namespace}/{source_name}/{key}")
        }
    };

    let secret = Secret {
        data: Some({
            let mut data = BTreeMap::default();
            data.insert("userID".into(), get_data("adminID")?);
            data.insert("userKey".into(), get_data("adminKey")?);
            data
        }),
        metadata: ObjectMeta {
            annotations: secret.metadata.annotations,
            labels: secret.metadata.labels,
            name: Some(target_name.into()),
            namespace: secret.metadata.namespace,
            ..Default::default()
        },
        type_: secret.type_,
        ..Default::default()
    };

    api.create(pp, &secret)
        .await
        .map(get_secret_ref)
        .map_err(|error| anyhow!(
            "failed to create a Rook CephFS user secret ({namespace}/{source_name} => {namespace}/{target_name}): {error}",
        ))
}
