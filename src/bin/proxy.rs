fn main() {
    let context = zmq::Context::new();
    let frontend = context.socket(zmq::ROUTER).unwrap();
    let backend = context.socket(zmq::DEALER).unwrap();
    assert!(frontend.bind("tcp://*:5559").is_ok());
    assert!(backend.bind("tcp://*:5560").is_ok());

    zmq::proxy(&frontend, &backend).unwrap();
}
