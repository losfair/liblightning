%module(directors="1") llbridge
%feature("director");

%{
    #include "../llbridge.hpp"
    #include "../llbridge_highlevel.hpp"
    using namespace liblightning;
    using namespace llbridge_highlevel;
%}

%include "../llbridge.hpp"
%include "../llbridge_highlevel.hpp"
