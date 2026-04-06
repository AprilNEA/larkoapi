fn main() {
    prost_build::Config::new()
        .compile_protos(&["proto/pbbp2.proto"], &["proto/"])
        .expect("failed to compile protobuf");
}
