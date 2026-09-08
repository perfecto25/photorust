# The shell's own tests.
#
# They link `photorust_shell`, so they exercise the same widgets the
# application runs rather than a second copy built with different flags. They
# need no window server: `QT_QPA_PLATFORM=offscreen` gives a real QApplication
# and real widgets with nowhere to put them, which is all these assertions want.

set(SHELL_TESTS
    tst_docktitle
    tst_filterdialog
    tst_glyphspanel
    tst_keymap
    tst_layerspanel
    tst_paragraphstyles
    tst_toolcursors
    tst_typeantialias
    tst_typerestyle
    tst_typewarp
    tst_workpath
)

foreach(test_name IN LISTS SHELL_TESTS)
    add_executable(${test_name} ${test_name}.cpp)
    target_link_libraries(${test_name} PRIVATE photorust_shell Qt::Test)
    target_compile_options(${test_name} PRIVATE
        $<$<CXX_COMPILER_ID:GNU,Clang,AppleClang>:-Wall -Wextra -Wpedantic>
    )
    add_test(NAME ${test_name} COMMAND ${test_name})
    set_tests_properties(${test_name} PROPERTIES
        ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
        TIMEOUT 120
    )
endforeach()
