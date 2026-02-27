//! Build script — compile Consumer Contract proto files (oneshim.client.v1).

fn main() {
    if std::env::var("CARGO_FEATURE_GRPC").is_ok() {
        compile_protos();
    }
}

fn compile_protos() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    // Consumer Contract protos live inside the client-rust workspace
    let proto_root = std::path::Path::new(&manifest_dir).join("../../api/proto");

    let protos: Vec<std::path::PathBuf> = vec![
        proto_root.join("oneshim/client/v1/auth.proto"),
        proto_root.join("oneshim/client/v1/session.proto"),
        proto_root.join("oneshim/client/v1/context.proto"),
        proto_root.join("oneshim/client/v1/suggestion.proto"),
        proto_root.join("oneshim/client/v1/health.proto"),
    ];

    let mut missing_protos = false;
    for proto in &protos {
        if !proto.exists() {
            eprintln!("Warning: Proto file not found: {:?}", proto);
            missing_protos = true;
        }
    }

    if missing_protos {
        eprintln!("Some proto files are missing. Skipping code generation.");
        return;
    }

    let out_dir = std::path::Path::new(&manifest_dir).join("src/proto/generated");
    std::fs::create_dir_all(&out_dir).expect("Failed to create output directory");

    tonic_prost_build::configure()
        .build_server(false)
        .build_client(true)
        .out_dir(&out_dir)
        .compile_protos(&protos, std::slice::from_ref(&proto_root))
        .expect("Failed to compile protos");

    // Patch generated code for tonic 0.12 compatibility.
    // tonic-build 0.14 emits `tonic::body::Body` and `tonic_prost::ProstCodec`
    // which belong to tonic 0.13+. Replace them with their tonic 0.12 equivalents.
    let generated = out_dir.join("oneshim.client.v1.rs");
    if generated.exists() {
        let content = std::fs::read_to_string(&generated).expect("read generated");
        let patched = content
            .replace("tonic::body::Body", "tonic::body::BoxBody")
            .replace("tonic_prost::ProstCodec", "tonic::codec::ProstCodec");
        std::fs::write(&generated, patched).expect("write patched");
    }

    for proto in &protos {
        println!("cargo:rerun-if-changed={}", proto.display());
    }
}
