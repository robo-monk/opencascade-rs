use std::path::Path;

use opencascade_sys::ffi;

use crate::{primitives::Shape, Error};

pub type StepTransform = [[f64; 4]; 4];

pub struct StepAssemblyNode {
    pub entry: String,
    pub parent_entry: Option<String>,
    pub referred_entry: Option<String>,
    pub name: String,
    pub local_transform: StepTransform,
    pub shape: Shape,
    pub is_assembly: bool,
    pub is_reference: bool,
    pub color: Option<[f64; 3]>,
}

pub struct StepAssemblyExportNode<'a> {
    pub name: &'a str,
    pub shape: &'a Shape,
    pub local_transform: StepTransform,
}

pub fn read_step_assembly(path: impl AsRef<Path>) -> Result<Vec<StepAssemblyNode>, Error> {
    let document = ffi::read_step_xde(path.as_ref().to_string_lossy().to_string());
    if document.is_null() {
        return Err(Error::StepReadFailed);
    }

    let count = ffi::xde_node_count(&document);
    let mut nodes = Vec::with_capacity(count);
    for index in 0..count {
        let shape = ffi::xde_node_shape(&document, index);
        nodes.push(StepAssemblyNode {
            entry: ffi::xde_node_entry(&document, index),
            parent_entry: empty_to_none(ffi::xde_node_parent_entry(&document, index)),
            referred_entry: empty_to_none(ffi::xde_node_referred_entry(&document, index)),
            name: ffi::xde_node_name(&document, index),
            local_transform: transform_from_location(&ffi::xde_node_location(&document, index)),
            shape: Shape::from_shape(&shape),
            is_assembly: ffi::xde_node_is_assembly(&document, index),
            is_reference: ffi::xde_node_is_reference(&document, index),
            color: color_from_xde_node(&document, index),
        });
    }
    Ok(nodes)
}

pub fn write_step_assembly(
    path: impl AsRef<Path>,
    root_name: &str,
    nodes: &[StepAssemblyExportNode<'_>],
) -> Result<(), Error> {
    let mut writer = ffi::step_assembly_writer_new(root_name.to_string());
    if writer.is_null() {
        return Err(Error::StepWriteFailed);
    }

    for node in nodes {
        if node.local_transform == identity_transform() {
            ffi::step_assembly_writer_add_shape(
                writer.pin_mut(),
                node.name.to_string(),
                &node.shape.inner,
            );
            continue;
        }

        let mut transform = ffi::new_transform();
        transform.pin_mut().SetValues(
            node.local_transform[0][0],
            node.local_transform[0][1],
            node.local_transform[0][2],
            node.local_transform[0][3],
            node.local_transform[1][0],
            node.local_transform[1][1],
            node.local_transform[1][2],
            node.local_transform[1][3],
            node.local_transform[2][0],
            node.local_transform[2][1],
            node.local_transform[2][2],
            node.local_transform[2][3],
        );
        ffi::step_assembly_writer_add_shape_located(
            writer.pin_mut(),
            node.name.to_string(),
            &node.shape.inner,
            &transform,
        );
    }

    if ffi::step_assembly_writer_write(
        writer.pin_mut(),
        path.as_ref().to_string_lossy().to_string(),
    ) {
        Ok(())
    } else {
        Err(Error::StepWriteFailed)
    }
}

fn empty_to_none(value: String) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn color_from_xde_node(document: &ffi::XdeDocument, index: usize) -> Option<[f64; 3]> {
    if !ffi::xde_node_has_color(document, index) {
        return None;
    }

    Some([
        ffi::xde_node_color_r(document, index),
        ffi::xde_node_color_g(document, index),
        ffi::xde_node_color_b(document, index),
    ])
}

fn transform_from_location(location: &cxx::UniquePtr<ffi::TopLoc_Location>) -> StepTransform {
    let Some(location) = location.as_ref() else {
        return identity_transform();
    };
    let transform = ffi::TopLoc_Location_Transformation(location);
    let Some(transform) = transform.as_ref() else {
        return identity_transform();
    };
    [
        [
            transform.Value(1, 1),
            transform.Value(1, 2),
            transform.Value(1, 3),
            transform.Value(1, 4),
        ],
        [
            transform.Value(2, 1),
            transform.Value(2, 2),
            transform.Value(2, 3),
            transform.Value(2, 4),
        ],
        [
            transform.Value(3, 1),
            transform.Value(3, 2),
            transform.Value(3, 3),
            transform.Value(3, 4),
        ],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn identity_transform() -> StepTransform {
    [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::Shape;
    use opencascade_sys::ffi;

    #[test]
    fn import_step_round_trip_assembly_keeps_names_and_locations() {
        let dir = temp_dir("occt-step-assembly");
        let path = dir.join("assembly.step");
        let cube = Shape::cube(1.0);
        let identity = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let translated = [
            [1.0, 0.0, 0.0, 5.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let nodes = [
            StepAssemblyExportNode { name: "left", shape: &cube, local_transform: identity },
            StepAssemblyExportNode { name: "right", shape: &cube, local_transform: translated },
        ];
        write_step_assembly(&path, "fixture", &nodes).unwrap();

        let imported = read_step_assembly(&path).unwrap();
        assert!(imported.iter().any(|entity| entity.name.contains("left")));
        let right = imported.iter().find(|entity| entity.name.contains("right")).unwrap();
        assert!((right.local_transform[0][3] - 5.0).abs() < 1e-6);

        std::fs::remove_file(&path).ok();
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn import_step_round_trip_assembly_resolves_referred_shape_color() {
        let dir = temp_dir("occt-step-color");
        let path = dir.join("color.step");
        let cube = Shape::cube(1.0);

        let mut writer = ffi::step_assembly_writer_new("fixture".to_string());
        assert!(!writer.is_null());
        ffi::step_assembly_writer_add_shape(writer.pin_mut(), "colored".to_string(), &cube.inner);
        assert!(ffi::step_assembly_writer_set_shape_color(
            writer.pin_mut(),
            &cube.inner,
            0.25,
            0.5,
            0.75
        ));
        assert!(ffi::step_assembly_writer_write(
            writer.pin_mut(),
            path.to_string_lossy().to_string()
        ));

        let imported = read_step_assembly(&path).unwrap();
        let colored = imported
            .iter()
            .find(|entity| entity.parent_entry.is_some() && entity.referred_entry.is_some())
            .expect("colored instance not found");
        let color = colored.color.expect("expected imported color");
        assert!((color[0] - 0.25).abs() < 1e-6);
        assert!((color[1] - 0.5).abs() < 1e-6);
        assert!((color[2] - 0.75).abs() < 1e-6);

        std::fs::remove_file(&path).ok();
        std::fs::remove_dir_all(&dir).ok();
    }

    fn temp_dir(prefix: &str) -> std::path::PathBuf {
        let unique = format!(
            "{prefix}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        );
        let dir = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
