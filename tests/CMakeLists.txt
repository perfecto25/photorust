# Test wiring.
#
# The image engine's own tests live next to the code they cover and run under
# Cargo; this registers them with CTest so `ctest` exercises both halves of the
# project in one command.

find_program(CARGO_EXECUTABLE cargo REQUIRED)

add_test(
    NAME core_unit_tests
    COMMAND ${CARGO_EXECUTABLE} test --manifest-path ${CMAKE_SOURCE_DIR}/core/Cargo.toml
)

# Cargo needs to find qmake to build the cxx-qt bridge, exactly as it does
# during the main build.
get_target_property(QMAKE_EXECUTABLE Qt::qmake IMPORTED_LOCATION)
set_tests_properties(core_unit_tests PROPERTIES
    ENVIRONMENT "QMAKE=${QMAKE_EXECUTABLE}"
    TIMEOUT 600
)
