# Spike addresses per browser build (RVA), found by hand for the builds on the test machine.
$sets = @{
 chrome  = 'detach 09FB8750;insert 09FB8240;vparent 198;vchildren 1A0;new 05A1E310;ctor 01D7D840;size 528;ctx 055938A0;add_child 054C1F40;set_contents 022A9840;set_bounds 04A52AF0;focus_behavior 00F32240;request_focus 026F9A70;dtor_slot 9;find_slot 01834670'
 brave   = 'vparent 198;vchildren 1A0;new 06F4DE00;ctor 0222A9B0;size 528;ctx 06AC7B40;add_child 06651DF0;set_contents 027540A0;set_bounds 05EE66C0;dtor_slot 9;focus_behavior 0085D450;request_focus 027542B0;bounds_slot 4B'
 edge    = 'detach 0A408856;insert 0A408332;vparent 198;vchildren 1A0;new 0370AEC0;ctor 02B3A51A;size 5B0;ctx 01192390;add_child 01105EAE;set_contents 02A7D730;set_bounds 01050418;dtor_slot 9;focus_behavior 0110C4C4;request_focus 02DD71A0'
 whale   = 'detach 08181E70;insert 081814D0;vparent 190;vchildren 198;new 03E4AB40;ctor 0E6849C0;size 538;ctxoff 268;add_child 08F86720;set_contents 0E684F30;set_bounds 08F7C700;focus_slot 31;dtor_slot 9'
 samsung = 'vparent 190;vchildren 198;new 055841C8;ctor 01AEB440;size 4C0;ctxoff 258;add_child 04C0BB50;set_contents 01FD2EA0;boundsoff 1C0;focus_slot 31;dtor_slot 9'
}
