'use strict';
'require view';
'require form';
'require uci';

return view.extend({
	load: function() {
		return uci.load('cheburprobe');
	},

	render: function() {
		var m, s, o;

		m = new form.Map('cheburprobe', _('Cheburprobe'),
			_('Configure the Cheburcheck dynamic network probe. Changes are applied to the service automatically.'));
		s = m.section(form.NamedSection, 'main', 'cheburprobe', _('Service settings'));
		s.addremove = false;

		o = s.option(form.Flag, 'enabled', _('Enable service'));
		o.rmempty = false;

		o = s.option(form.Value, 'probe_id', _('Probe ID'));
		o.datatype = 'uinteger';
		o.rmempty = false;

		o = s.option(form.Value, 'probe_token', _('Probe token'));
		o.password = true;
		o.rmempty = false;

		o = s.option(form.Value, 'mqtt_host', _('MQTT WebSocket URL'));
		o.default = 'wss://cheburcheck.ru/mqtt';
		o.rmempty = false;

		o = s.option(form.Value, 'mqtt_port', _('MQTT port'));
		o.datatype = 'port';
		o.default = '443';
		o.rmempty = false;

		o = s.option(form.Value, 'connection_timeout', _('Connection timeout'), _('Seconds'));
		o.datatype = 'and(uinteger,min(1))';
		o.default = '30';

		o = s.option(form.Value, 'max_concurrent_tasks', _('Maximum concurrent tasks'));
		o.datatype = 'and(uinteger,min(1))';
		o.default = '8';

		o = s.option(form.Value, 'traceroute_max_hops', _('Traceroute maximum hops'));
		o.datatype = 'and(uinteger,min(1))';
		o.default = '5';

		o = s.option(form.Value, 'traceroute_retries', _('Traceroute retries'));
		o.datatype = 'and(uinteger,min(1))';
		o.default = '3';

		o = s.option(form.Value, 'traceroute_control_hosts', _('Traceroute control hosts'));
		o.datatype = 'and(uinteger,min(1))';
		o.default = '3';

		o = s.option(form.ListValue, 'log_level', _('Log level'));
		o.value('error', _('Error'));
		o.value('warn', _('Warning'));
		o.value('info', _('Info'));
		o.value('debug', _('Debug'));
		o.value('trace', _('Trace'));
		o.default = 'info';

		return m.render();
	}
});
